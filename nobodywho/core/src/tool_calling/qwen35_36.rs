use super::{Tool, ToolCall, ToolFormatError, ToolFormatHandler};
use gbnf::builder::{alt, nt, nt_plus, seq, t, GrammarBuilder};
use gbnf::json::json_schema_to_grammar;
use gbnf::{CharacterRange, Expr, GbnfGrammar, Quantifier};
use regex::Regex;
use serde_json::{Map, Value};
use tracing::debug;

const BEGIN_TOKEN: &str = "<tool_call>";
const END_TOKEN: &str = "</tool_call>";

/// Terminator of a `<parameter=...>` block, as emitted by the Qwen3.5/3.6 chat template.
///
/// The leading `\n` separates the value content from the close; the trailing `\n`
/// precedes either the next `<parameter=...>` or `</function>`.
const PARAM_TERMINATOR: &str = "\n</parameter>\n";

#[derive(Debug, Clone, Copy)]
pub struct Qwen35_36Handler;

/// Single-char expression that matches anything except the given chars.
fn none_of(chars: &[char]) -> Expr {
    Expr::CharacterRange(CharacterRange::Set {
        chars: chars.to_vec(),
        negated: true,
    })
}

/// Add GBNF rules for an arbitrary-content body terminated by `PARAM_TERMINATOR`.
///
/// Grammar is a KMP-style automaton over the terminator so the parser correctly
/// handles the repeated `\n` at positions 0 and 13 of `"\n</parameter>\n"`. Because
/// that is the only repeated character in the terminator, the failure function
/// always resets to state `a0` on a stray `\n`.
///
/// Returns the name of the body rule.
fn add_param_value_body_rules(
    prefix: &str,
    mut b: GrammarBuilder<gbnf::builder::NoRoot>,
) -> (String, GrammarBuilder<gbnf::builder::NoRoot>) {
    // terminator chars (by index): 0=\n, 1=<, 2=/, 3=p, 4=a, 5=r, 6=a, 7=m, 8=e, 9=t, 10=e, 11=r, 12=>, 13=\n
    // intermediate states a0..a12 represent "consumed c_0..c_k"; a13 would be terminator-complete.
    let mid_chars: [char; 12] = ['<', '/', 'p', 'a', 'r', 'a', 'm', 'e', 't', 'e', 'r', '>'];

    let body = format!("{prefix}-body");
    let a = |k: usize| format!("{prefix}-a{k}");

    // body ::= "" | [^\n] body | "\n" a0
    b = b.rule(
        &body,
        alt(&[
            t(""),
            seq(&[none_of(&['\n']), nt(&body)]),
            seq(&[t("\n"), nt(&a(0))]),
        ]),
    );

    // a0..a11 ::= "" | <next_c> a{k+1} | "\n" a0 | [^\n, next_c] body
    for (k, &next_c) in mid_chars.iter().enumerate() {
        debug_assert_ne!(next_c, '\n');
        let next_state = a(k + 1);
        b = b.rule(
            &a(k),
            alt(&[
                t(""),
                seq(&[t(&next_c.to_string()), nt(&next_state)]),
                seq(&[t("\n"), nt(&a(0))]),
                seq(&[none_of(&['\n', next_c]), nt(&body)]),
            ]),
        );
    }

    // a12 ::= "" | "\n" a0 | [^\n] body
    // No explicit c_13 case: consuming the final "\n" completes the terminator, which the
    // outer rule handles. The epsilon alternative lets the parser exit the body so the
    // outer terminator can match.
    b = b.rule(
        &a(12),
        alt(&[
            t(""),
            seq(&[t("\n"), nt(&a(0))]),
            seq(&[none_of(&['\n']), nt(&body)]),
        ]),
    );

    (body, b)
}

/// Build a rule matching a single parameter value per its JSON schema type.
///
/// Strings share the single body-terminator rule (`body_rule`). Scalars use literal
/// GBNF. Objects/arrays delegate to `json_schema_to_grammar`, which is included into
/// this grammar under a uniquified alias.
fn add_value_rule(
    schema: &Value,
    prefix: &str,
    body_rule: &str,
    mut b: GrammarBuilder<gbnf::builder::NoRoot>,
) -> Result<(String, GrammarBuilder<gbnf::builder::NoRoot>), ToolFormatError> {
    let ty = schema
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("string");
    match ty {
        "string" => {
            if let Some(variants) = schema.get("enum").and_then(|e| e.as_array()) {
                let alts: Vec<Expr> = variants.iter().filter_map(|v| v.as_str()).map(t).collect();
                if !alts.is_empty() {
                    let r = format!("{prefix}-enum");
                    b = b.rule(&r, alt(&alts));
                    return Ok((r, b));
                }
            }
            Ok((body_rule.to_string(), b))
        }
        "boolean" => {
            let r = format!("{prefix}-bool");
            b = b.rule(&r, alt(&[t("true"), t("false")]));
            Ok((r, b))
        }
        "null" => {
            let r = format!("{prefix}-null");
            b = b.rule(&r, t("null"));
            Ok((r, b))
        }
        "integer" => {
            let digit = format!("{prefix}-digit");
            let r = format!("{prefix}-int");
            b = b.rule(
                &digit,
                Expr::CharacterRange(CharacterRange::Range {
                    begin: '0',
                    end: '9',
                    negated: false,
                }),
            );
            b = b.rule(
                &r,
                seq(&[
                    Expr::Quantified {
                        expr: Box::new(t("-")),
                        quantifier: Quantifier::Optional,
                    },
                    Expr::Quantified {
                        expr: Box::new(nt(&digit)),
                        quantifier: Quantifier::OneOrMore,
                    },
                ]),
            );
            Ok((r, b))
        }
        "number" => {
            let digit = format!("{prefix}-digit");
            let r = format!("{prefix}-num");
            b = b.rule(
                &digit,
                Expr::CharacterRange(CharacterRange::Range {
                    begin: '0',
                    end: '9',
                    negated: false,
                }),
            );
            let digits = Expr::Quantified {
                expr: Box::new(nt(&digit)),
                quantifier: Quantifier::OneOrMore,
            };
            b = b.rule(
                &r,
                seq(&[
                    Expr::Quantified {
                        expr: Box::new(t("-")),
                        quantifier: Quantifier::Optional,
                    },
                    digits.clone(),
                    Expr::Quantified {
                        expr: Box::new(seq(&[t("."), digits])),
                        quantifier: Quantifier::Optional,
                    },
                ]),
            );
            Ok((r, b))
        }
        "object" | "array" => {
            let alias = format!("{prefix}-json");
            let json_gram = json_schema_to_grammar(schema.clone(), "root")?;
            b = b.include_grammar_as(&json_gram, &alias);
            Ok((alias, b))
        }
        _ => Ok((body_rule.to_string(), b)),
    }
}

impl ToolFormatHandler for Qwen35_36Handler {
    fn begin_token(&self) -> &str {
        BEGIN_TOKEN
    }

    fn end_token(&self) -> &str {
        END_TOKEN
    }

    fn generate_grammar(&self, tools: &[Tool]) -> Result<GbnfGrammar, ToolFormatError> {
        let mut builder = GrammarBuilder::new();
        let (body_rule, b) = add_param_value_body_rules("qwen35-val", builder);
        builder = b;

        let mut tool_rule_names = Vec::new();
        for (ti, tool) in tools.iter().enumerate() {
            let tprefix = format!("qwen35-t{ti}");

            let required: std::collections::HashSet<&str> = tool
                .json_schema
                .get("required")
                .and_then(|r| r.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();

            let mut param_exprs: Vec<Expr> = Vec::new();
            if let Some(props) = tool
                .json_schema
                .get("properties")
                .and_then(|p| p.as_object())
            {
                for (pname, pschema) in props.iter() {
                    let pprefix = format!("{tprefix}-p-{}", sanitize(pname));
                    let (value_rule, b) = add_value_rule(pschema, &pprefix, &body_rule, builder)?;
                    builder = b;

                    let param_rule = format!("{pprefix}-block");
                    builder = builder.rule(
                        &param_rule,
                        seq(&[
                            t(&format!("<parameter={pname}>\n")),
                            nt(&value_rule),
                            t(PARAM_TERMINATOR),
                        ]),
                    );
                    let param_expr = if required.contains(pname.as_str()) {
                        nt(&param_rule)
                    } else {
                        Expr::Quantified {
                            expr: Box::new(nt(&param_rule)),
                            quantifier: Quantifier::Optional,
                        }
                    };
                    param_exprs.push(param_expr);
                }
            }

            let tool_rule = format!("{tprefix}-call");
            let mut call_seq: Vec<Expr> = Vec::with_capacity(param_exprs.len() + 2);
            call_seq.push(t(&format!("<function={}>\n", tool.name)));
            call_seq.extend(param_exprs);
            call_seq.push(t("</function>\n"));
            builder = builder.rule(&tool_rule, seq(&call_seq));
            tool_rule_names.push(tool_rule);
        }

        let tool_alts: Vec<Expr> = tool_rule_names.iter().map(|n| nt(n)).collect();
        let grammar = builder
            .rule("tool-alt", alt(&tool_alts))
            .rule(
                "toolcall",
                seq(&[t(BEGIN_TOKEN), t("\n"), nt("tool-alt"), t(END_TOKEN)]),
            )
            .rule("superroot", nt_plus("toolcall"))
            .root("superroot")
            .build();

        Ok(grammar)
    }

    fn extract_tool_calls(&self, input: &str) -> Option<Vec<ToolCall>> {
        let outer = Regex::new(
            r"(?s)<tool_call>\s*<function=([^>\s]+)>\s*(.*?)\s*</function>\s*</tool_call>",
        )
        .ok()?;
        let param_re = Regex::new(r"(?s)<parameter=([^>\s]+)>\n(.*?)\n</parameter>").ok()?;

        let mut calls = Vec::new();
        for cap in outer.captures_iter(input) {
            let name = cap.get(1)?.as_str().to_string();
            let body = cap.get(2)?.as_str();
            let mut arguments: Map<String, Value> = Map::new();
            for p in param_re.captures_iter(body) {
                let key = p.get(1)?.as_str().to_string();
                let raw = p.get(2)?.as_str();
                let value = serde_json::from_str::<Value>(raw)
                    .unwrap_or_else(|_| Value::String(raw.to_string()));
                arguments.insert(key, value);
            }
            debug!(tool_name = %name, ?arguments, "parsed Qwen3.5 tool call");
            calls.push(ToolCall {
                name,
                arguments: Value::Object(arguments),
            });
        }

        if calls.is_empty() {
            None
        } else {
            Some(calls)
        }
    }
}

/// Conservative rule-name sanitizer: GBNF rule names allow alphanumerics, `-`, and `_`.
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
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
}
