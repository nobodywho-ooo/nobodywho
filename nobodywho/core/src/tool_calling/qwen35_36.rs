use super::{Tool, ToolCall, ToolFormatError, ToolFormatHandler};
use gbnf::builder::{alt, nt, nt_plus, seq, t, GrammarBuilder, NoRoot};
use gbnf::json::json_schema_to_grammar;
use gbnf::{CharacterRange, Expr, GbnfGrammar, Quantifier};
use regex::Regex;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::sync::OnceLock;
use tracing::debug;

const BEGIN_TOKEN: &str = "<tool_call>";
const END_TOKEN: &str = "</tool_call>";

/// Terminator of a `<parameter=...>` block, as emitted by the Qwen3.5/3.6 chat template.
///
/// The leading `\n` separates the value content from the close; the trailing `\n`
/// precedes either the next `<parameter=...>` or `</function>`.
const PARAM_TERMINATOR: &str = "\n</parameter>\n";

type Builder = GrammarBuilder<NoRoot>;

#[derive(Debug, Clone, Copy)]
pub struct Qwen35_36Handler;

impl ToolFormatHandler for Qwen35_36Handler {
    fn begin_token(&self) -> &str {
        BEGIN_TOKEN
    }

    fn end_token(&self) -> &str {
        END_TOKEN
    }

    fn generate_grammar(&self, tools: &[Tool]) -> Result<GbnfGrammar, ToolFormatError> {
        let (body_rule, mut builder) =
            add_param_value_body_rule("qwen35-val", GrammarBuilder::new());

        let mut tool_rule_names = Vec::with_capacity(tools.len());
        for (ti, tool) in tools.iter().enumerate() {
            let (tool_rule, next) = add_tool_rule(tool, ti, &body_rule, builder)?;
            builder = next;
            tool_rule_names.push(tool_rule);
        }

        let tool_alts: Vec<Expr> = tool_rule_names.iter().map(|n| nt(n)).collect();
        Ok(builder
            .rule("tool-alt", alt(&tool_alts))
            .rule(
                "toolcall",
                seq(&[t(BEGIN_TOKEN), t("\n"), nt("tool-alt"), t(END_TOKEN)]),
            )
            .rule("superroot", nt_plus("toolcall"))
            .root("superroot")
            .build())
    }

    fn extract_tool_calls(&self, input: &str) -> Option<Vec<ToolCall>> {
        let calls: Vec<ToolCall> = outer_tool_call_regex()
            .captures_iter(input)
            .filter_map(parse_tool_call_capture)
            .collect();

        (!calls.is_empty()).then_some(calls)
    }
}

// ============================================================================
// Grammar construction
// ============================================================================
//
// Each `add_*` helper receives the in-progress `Builder`, appends one or more
// rules, and returns the new `Builder` together with the name of the rule the
// caller should reference.

fn none_of(chars: &[char]) -> Expr {
    Expr::CharacterRange(CharacterRange::Set {
        chars: chars.to_vec(),
        negated: true,
    })
}

/// Add GBNF rules for an arbitrary-content body that cannot consume
/// `PARAM_TERMINATOR` (`\n</parameter>\n`).
///
/// The terminator always begins with `\n<`, so the body allows multi-line
/// content but forbids `<` at the start of any line. This is two rules
/// instead of the 14-state KMP automaton needed to match the full terminator
/// string, and is sufficient in practice because tool-call parameter values
/// (code, text, etc.) rarely start a line with `<`.
fn add_param_value_body_rule(prefix: &str, mut b: Builder) -> (String, Builder) {
    let body = format!("{prefix}-body");
    let after_nl = format!("{prefix}-after-nl");

    // body ::= "" | [^\n] body | "\n" after-nl
    b = b.rule(
        &body,
        alt(&[
            t(""),
            seq(&[none_of(&['\n']), nt(&body)]),
            seq(&[t("\n"), nt(&after_nl)]),
        ]),
    );

    // after-nl ::= "" | [^<\n] body | "\n" after-nl
    b = b.rule(
        &after_nl,
        alt(&[
            t(""),
            seq(&[none_of(&['<', '\n']), nt(&body)]),
            seq(&[t("\n"), nt(&after_nl)]),
        ]),
    );

    (body, b)
}

/// Build a rule matching a single parameter value per its JSON schema type.
fn add_value_rule(
    schema: &Value,
    prefix: &str,
    body_rule: &str,
    b: Builder,
) -> Result<(String, Builder), ToolFormatError> {
    let ty = schema
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("string");

    if ty != "string" {
        let alias = format!("{prefix}-json");
        let json_gram = json_schema_to_grammar(schema.clone(), "root")?;
        return Ok((alias.clone(), b.include_grammar_as(&json_gram, &alias)));
    }

    if let Some(variants) = schema.get("enum").and_then(|e| e.as_array()) {
        let alts: Vec<Expr> = variants.iter().filter_map(|v| v.as_str()).map(t).collect();
        if !alts.is_empty() {
            let rule_name = format!("{prefix}-enum");
            return Ok((rule_name.clone(), b.rule(&rule_name, alt(&alts))));
        }
    }

    Ok((body_rule.to_string(), b))
}

fn required_params(tool: &Tool) -> HashSet<&str> {
    tool.json_schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default()
}

fn add_parameter_rule(
    pname: &str,
    pschema: &Value,
    tprefix: &str,
    body_rule: &str,
    required: &HashSet<&str>,
    builder: Builder,
) -> Result<(Expr, Builder), ToolFormatError> {
    let pprefix = format!("{tprefix}-p-{}", sanitize(pname));
    let (value_rule, builder) = add_value_rule(pschema, &pprefix, body_rule, builder)?;

    let param_rule = format!("{pprefix}-block");
    let builder = builder.rule(
        &param_rule,
        seq(&[
            t(&format!("<parameter={pname}>\n")),
            nt(&value_rule),
            t(PARAM_TERMINATOR),
        ]),
    );

    let param_expr = if required.contains(pname) {
        nt(&param_rule)
    } else {
        Expr::Quantified {
            expr: Box::new(nt(&param_rule)),
            quantifier: Quantifier::Optional,
        }
    };

    Ok((param_expr, builder))
}

fn add_tool_rule(
    tool: &Tool,
    tool_index: usize,
    body_rule: &str,
    mut builder: Builder,
) -> Result<(String, Builder), ToolFormatError> {
    let tprefix = format!("qwen35-t{tool_index}");
    let required = required_params(tool);
    let mut param_exprs: Vec<Expr> = Vec::new();

    if let Some(props) = tool
        .json_schema
        .get("properties")
        .and_then(|p| p.as_object())
    {
        for (pname, pschema) in props {
            let (expr, next) =
                add_parameter_rule(pname, pschema, &tprefix, body_rule, &required, builder)?;
            builder = next;
            param_exprs.push(expr);
        }
    }

    let tool_rule = format!("{tprefix}-call");
    let mut call_seq: Vec<Expr> = Vec::with_capacity(param_exprs.len() + 2);
    call_seq.push(t(&format!("<function={}>\n", tool.name)));
    call_seq.extend(param_exprs);
    call_seq.push(t("</function>\n"));

    Ok((tool_rule.clone(), builder.rule(&tool_rule, seq(&call_seq))))
}

/// llama.cpp's grammar parser only accepts `[a-zA-Z0-9-]` in rule names
/// (see `is_word_char` in `llama-grammar.cpp`), so underscores in parameter
/// names must be mapped to `-`.
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

// ============================================================================
// Parsing
// ============================================================================

fn outer_tool_call_regex() -> &'static Regex {
    static OUTER: OnceLock<Regex> = OnceLock::new();
    OUTER.get_or_init(|| {
        Regex::new(r"(?s)<tool_call>\s*<function=([^>\s]+)>\s*(.*?)\s*</function>\s*</tool_call>")
            .expect("outer Qwen3.5/3.6 tool regex should compile")
    })
}

fn parameter_regex() -> &'static Regex {
    static PARAM: OnceLock<Regex> = OnceLock::new();
    PARAM.get_or_init(|| {
        Regex::new(r"(?s)<parameter=([^>\s]+)>\n(.*?)\n</parameter>")
            .expect("parameter Qwen3.5/3.6 regex should compile")
    })
}

fn parse_parameter_arguments(body: &str) -> Map<String, Value> {
    parameter_regex()
        .captures_iter(body)
        .filter_map(|p| {
            let key = p.get(1)?.as_str().to_string();
            let raw = p.get(2)?.as_str();
            let value = serde_json::from_str::<Value>(raw)
                .unwrap_or_else(|_| Value::String(raw.to_string()));
            Some((key, value))
        })
        .collect()
}

fn parse_tool_call_capture(cap: regex::Captures<'_>) -> Option<ToolCall> {
    let name = cap.get(1)?.as_str().to_string();
    let body = cap.get(2)?.as_str();
    let arguments = parse_parameter_arguments(body);
    debug!(tool_name = %name, ?arguments, "parsed Qwen3.5 tool call");

    Some(ToolCall {
        name,
        arguments: Value::Object(arguments),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tokens() {
        let h = Qwen35_36Handler;
        assert_eq!(h.begin_token(), "<tool_call>");
        assert_eq!(h.end_token(), "</tool_call>");
    }

    #[test]
    fn extract_single_string_param() {
        let h = Qwen35_36Handler;
        let input = "<tool_call>\n<function=get_weather>\n<parameter=city>\nCopenhagen\n</parameter>\n</function>\n</tool_call>";
        let calls = h.extract_tool_calls(input).expect("should parse");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "get_weather");
        assert_eq!(calls[0].arguments, json!({"city": "Copenhagen"}));
    }

    #[test]
    fn extract_scalar_params() {
        let h = Qwen35_36Handler;
        let input = "<tool_call>\n<function=f>\n<parameter=n>\n42\n</parameter>\n<parameter=b>\ntrue\n</parameter>\n</function>\n</tool_call>";
        let calls = h.extract_tool_calls(input).expect("should parse");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].arguments, json!({"n": 42, "b": true}));
    }

    #[test]
    fn extract_object_param() {
        let h = Qwen35_36Handler;
        let input = "<tool_call>\n<function=f>\n<parameter=opts>\n{\"a\": 1, \"b\": [2, 3]}\n</parameter>\n</function>\n</tool_call>";
        let calls = h.extract_tool_calls(input).expect("should parse");
        assert_eq!(calls[0].arguments, json!({"opts": {"a": 1, "b": [2, 3]}}));
    }

    #[test]
    fn extract_multiple_calls() {
        let h = Qwen35_36Handler;
        let input = "<tool_call>\n<function=one>\n<parameter=x>\n1\n</parameter>\n</function>\n</tool_call><tool_call>\n<function=two>\n<parameter=y>\nhi\n</parameter>\n</function>\n</tool_call>";
        let calls = h.extract_tool_calls(input).expect("should parse");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "one");
        assert_eq!(calls[1].name, "two");
        assert_eq!(calls[1].arguments, json!({"y": "hi"}));
    }

    #[test]
    fn extract_no_calls() {
        let h = Qwen35_36Handler;
        assert!(h.extract_tool_calls("plain text").is_none());
    }

    #[test]
    fn grammar_builds_for_typical_schema() {
        let h = Qwen35_36Handler;
        let tool = Tool {
            name: "get_weather".to_string(),
            description: "Get weather".to_string(),
            json_schema: json!({
                "type": "object",
                "properties": {
                    "city": {"type": "string"},
                    "units": {"type": "string", "enum": ["celsius", "fahrenheit"]},
                    "verbose": {"type": "boolean"}
                },
                "required": ["city"]
            }),
            function: std::sync::Arc::new(|_| "".to_string()),
        };
        let gram = h.generate_grammar(&[tool]).expect("grammar should build");
        let s = gram.as_str();
        assert!(s.contains("<tool_call>"));
        assert!(s.contains("<function=get_weather>"));
        assert!(s.contains("<parameter=city>"));
    }

    #[test]
    fn grammar_reuses_json_scalar_rules_for_non_strings() {
        let h = Qwen35_36Handler;
        let tool = Tool {
            name: "f".to_string(),
            description: "test".to_string(),
            json_schema: json!({
                "type": "object",
                "properties": {
                    "n": {"type": "integer"},
                    "x": {"type": "number"},
                    "b": {"type": "boolean"},
                    "z": {"type": "null"}
                }
            }),
            function: std::sync::Arc::new(|_| "".to_string()),
        };

        let grammar = h.generate_grammar(&[tool]).expect("grammar should build");
        let s = grammar.as_str();
        assert!(s.contains("json-integer"));
        assert!(s.contains("json-number"));
        assert!(s.contains("json-boolean"));
        assert!(s.contains("json-null"));
    }
}
