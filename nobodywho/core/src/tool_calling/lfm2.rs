//! LFM2 tool calling. The model emits a Python-style call list
//! `<|tool_call_start|>[get_weather(location="Paris")]<|tool_call_end|>` — calls
//! separated by `, `, args as `key=value` (strings double-quoted, everything else
//! JSON-ish). We grammar-constrain to that shape and parse values leniently
//! (also tolerating Python `True`/`False`/`None`).

use super::{Tool, ToolCall, ToolFormatError, ToolFormatHandler};
use gbnf::builder::{alt, nt, seq, t, GrammarBuilder, NoRoot};
use gbnf::json::json_schema_to_grammar;
use gbnf::{Expr, GbnfGrammar};
use serde_json::{Map, Value};
use std::collections::HashSet;
use tracing::debug;

const BEGIN_TOKEN: &str = "<|tool_call_start|>";
const END_TOKEN: &str = "<|tool_call_end|>";

type Builder = GrammarBuilder<NoRoot>;

#[derive(Debug, Clone, Copy)]
pub struct Lfm2Handler;

impl ToolFormatHandler for Lfm2Handler {
    fn begin_token(&self) -> &str {
        BEGIN_TOKEN
    }

    fn end_token(&self) -> &str {
        END_TOKEN
    }

    fn generate_grammar(&self, tools: &[Tool]) -> Result<GbnfGrammar, ToolFormatError> {
        let mut builder = GrammarBuilder::new();

        let mut tool_rule_names = Vec::with_capacity(tools.len());
        for (ti, tool) in tools.iter().enumerate() {
            let (tool_rule, next) = add_tool_rule(tool, ti, builder)?;
            builder = next;
            tool_rule_names.push(tool_rule);
        }

        let tool_alts: Vec<Expr> = tool_rule_names.iter().map(|n| nt(n)).collect();

        Ok(builder
            .rule("lfm2-tool-alt", alt(&tool_alts))
            // A call list is one or more calls separated by ", ".
            // calllist ::= tool-alt | tool-alt ", " calllist
            .rule(
                "lfm2-calllist",
                alt(&[
                    nt("lfm2-tool-alt"),
                    seq(&[nt("lfm2-tool-alt"), t(", "), nt("lfm2-calllist")]),
                ]),
            )
            .rule(
                "lfm2-toolcall",
                seq(&[
                    t(BEGIN_TOKEN),
                    t("["),
                    nt("lfm2-calllist"),
                    t("]"),
                    t(END_TOKEN),
                ]),
            )
            .rule("superroot", nt("lfm2-toolcall"))
            .root("superroot")
            .build())
    }

    fn slice_regexes(&self) -> Vec<String> {
        vec![r#"[^"\\\x00-\x1F\x7F]+"#.to_string()]
    }

    fn extract_tool_calls(&self, input: &str) -> Option<Vec<ToolCall>> {
        // Locate the call block. The end token may be absent if the model was
        // cut off, in which case we parse to the end of the input.
        let start = input.find(BEGIN_TOKEN)?;
        let after = &input[start + BEGIN_TOKEN.len()..];
        let body = match after.find(END_TOKEN) {
            Some(end) => &after[..end],
            None => after,
        };

        // Strip the surrounding `[ ... ]`, tolerating their absence.
        let body = body.trim();
        let body = body.strip_prefix('[').unwrap_or(body);
        let body = body.strip_suffix(']').unwrap_or(body);

        let calls: Vec<ToolCall> = split_top_level(body, ',')
            .into_iter()
            .filter_map(|call| parse_one_call(call.trim()))
            .collect();

        (!calls.is_empty()).then_some(calls)
    }
}

// ============================================================================
// Grammar construction
// ============================================================================
//
// Mirrors the per-tool / per-parameter approach in `qwen35_36.rs`: each helper
// appends rules to the in-progress `Builder` and returns the new `Builder`
// together with the name of the rule the caller should reference.

fn required_params(tool: &Tool) -> HashSet<&str> {
    tool.json_schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default()
}

/// llama.cpp's grammar parser only accepts `[a-zA-Z0-9-]` in rule names, so any
/// other character in a parameter name is mapped to `-` (same as qwen35_36).
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Add a rule matching a single argument value. LFM2 quotes strings and renders
/// everything else JSON-style, so the JSON value grammar covers every type
/// (string, number, boolean, enum, object, array).
fn add_value_rule(
    schema: &Value,
    prefix: &str,
    b: Builder,
) -> Result<(String, Builder), ToolFormatError> {
    let alias = format!("{prefix}-val");
    let json_gram = json_schema_to_grammar(schema.clone(), "root")?;
    Ok((alias.clone(), b.include_grammar_as(&json_gram, &alias)))
}

/// Build the ordered, comma-separated `key=value` argument list. Required
/// params must appear, optional params may be skipped, and commas are placed
/// only *between* emitted params (no leading/trailing/double commas).
///
/// Encoded with two rule families over the parameter suffix `i..n`:
/// - `from-i`: nothing emitted yet (the first emitted param has no comma)
/// - `more-i`: a param was already emitted (every further param gets ", ")
///
/// Returns the entry rule (`from-0`), which also matches the empty list.
fn add_arglist_rules(
    tprefix: &str,
    params: &[(String, bool)],
    mut b: Builder,
) -> (String, Builder) {
    let n = params.len();

    // Base cases: nothing left to emit.
    b = b.rule(&format!("{tprefix}-from-{n}"), t(""));
    b = b.rule(&format!("{tprefix}-more-{n}"), t(""));

    for i in (0..n).rev() {
        let (kv_rule, required) = &params[i];
        let from_i = format!("{tprefix}-from-{i}");
        let more_i = format!("{tprefix}-more-{i}");
        let from_next = format!("{tprefix}-from-{}", i + 1);
        let more_next = format!("{tprefix}-more-{}", i + 1);

        if *required {
            // from-i  ::= kv more-(i+1)
            // more-i  ::= ", " kv more-(i+1)
            b = b.rule(&from_i, seq(&[nt(kv_rule), nt(&more_next)]));
            b = b.rule(&more_i, seq(&[t(", "), nt(kv_rule), nt(&more_next)]));
        } else {
            // from-i  ::= (kv more-(i+1)) | from-(i+1)
            // more-i  ::= (", " kv more-(i+1)) | more-(i+1)
            b = b.rule(
                &from_i,
                alt(&[seq(&[nt(kv_rule), nt(&more_next)]), nt(&from_next)]),
            );
            b = b.rule(
                &more_i,
                alt(&[seq(&[t(", "), nt(kv_rule), nt(&more_next)]), nt(&more_next)]),
            );
        }
    }

    (format!("{tprefix}-from-0"), b)
}

fn add_tool_rule(
    tool: &Tool,
    tool_index: usize,
    mut builder: Builder,
) -> Result<(String, Builder), ToolFormatError> {
    let tprefix = format!("lfm2-t{tool_index}");
    let required = required_params(tool);

    // (kv-rule-name, is-required) per property, preserving declaration order.
    let mut params: Vec<(String, bool)> = Vec::new();
    if let Some(props) = tool
        .json_schema
        .get("properties")
        .and_then(|p| p.as_object())
    {
        for (pname, pschema) in props {
            let pprefix = format!("{tprefix}-p-{}", sanitize(pname));
            let (value_rule, next) = add_value_rule(pschema, &pprefix, builder)?;
            builder = next;

            let kv_rule = format!("{pprefix}-kv");
            builder = builder.rule(&kv_rule, seq(&[t(&format!("{pname}=")), nt(&value_rule)]));
            params.push((kv_rule, required.contains(pname.as_str())));
        }
    }

    let (arglist_rule, next) = add_arglist_rules(&tprefix, &params, builder);
    builder = next;

    let tool_rule = format!("{tprefix}-call");
    let builder = builder.rule(
        &tool_rule,
        seq(&[t(&format!("{}(", tool.name)), nt(&arglist_rule), t(")")]),
    );

    Ok((tool_rule, builder))
}

// ============================================================================
// Parsing
// ============================================================================

/// Split `s` on `delim`, but only at the top level — ignoring `delim` inside
/// `"..."` strings (respecting `\` escapes) and inside `()`/`[]`/`{}` nesting.
fn split_top_level(s: &str, delim: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut escaped = false;
    let mut start = 0;

    for (i, c) in s.char_indices() {
        if in_str {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            c if c == delim && depth == 0 => {
                parts.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

/// Split `s` at the first top-level `=` (ignoring `=` inside strings/nesting).
fn split_first_eq(s: &str) -> Option<(&str, &str)> {
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut escaped = false;

    for (i, c) in s.char_indices() {
        if in_str {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '=' if depth == 0 => return Some((&s[..i], &s[i + 1..])),
            _ => {}
        }
    }
    None
}

/// Parse a single `name(key=value, ...)` call.
fn parse_one_call(s: &str) -> Option<ToolCall> {
    let open = s.find('(')?;
    let name = s[..open].trim();
    if name.is_empty() {
        return None;
    }
    let rest = &s[open + 1..];
    let close = rest.rfind(')')?;
    let argbody = &rest[..close];

    let mut arguments = Map::new();
    for seg in split_top_level(argbody, ',') {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        if let Some((key, raw_value)) = split_first_eq(seg) {
            arguments.insert(key.trim().to_string(), parse_value(raw_value.trim()));
        }
    }

    debug!(tool_name = %name, ?arguments, "parsed LFM2 tool call");
    Some(ToolCall {
        name: name.to_string(),
        arguments: Value::Object(arguments),
    })
}

/// Parse an argument value. Tries JSON first (covers `"str"`, numbers, `true`/
/// `false`/`null`, objects, arrays), then falls back to Python literals and
/// finally a bare string.
fn parse_value(raw: &str) -> Value {
    if raw.is_empty() {
        return Value::String(String::new());
    }
    if let Ok(v) = serde_json::from_str::<Value>(raw) {
        return v;
    }
    match raw {
        "True" => return Value::Bool(true),
        "False" => return Value::Bool(false),
        "None" => return Value::Null,
        _ => {}
    }
    // Python-style single-quoted string.
    if raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'') {
        return Value::String(raw[1..raw.len() - 1].to_string());
    }
    Value::String(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tokens() {
        let h = Lfm2Handler;
        assert_eq!(h.begin_token(), "<|tool_call_start|>");
        assert_eq!(h.end_token(), "<|tool_call_end|>");
    }

    #[test]
    fn extract_value_types() {
        let h = Lfm2Handler;
        let args = |s| h.extract_tool_calls(s).unwrap()[0].arguments.clone();

        let calls = h
            .extract_tool_calls(
                "<|tool_call_start|>[get_weather(location=\"Paris\")]<|tool_call_end|>",
            )
            .unwrap();
        assert_eq!(calls[0].name, "get_weather");
        assert_eq!(calls[0].arguments, json!({"location": "Paris"}));

        // scalars, nested JSON object/array, and Python literals
        assert_eq!(
            args("<|tool_call_start|>[f(n=42, x=3.5, b=true, z=null)]<|tool_call_end|>"),
            json!({"n": 42, "x": 3.5, "b": true, "z": null})
        );
        assert_eq!(
            args("<|tool_call_start|>[f(opts={\"a\": 1, \"b\": [2, 3]}, xs=[1, 2])]<|tool_call_end|>"),
            json!({"opts": {"a": 1, "b": [2, 3]}, "xs": [1, 2]})
        );
        assert_eq!(
            args("<|tool_call_start|>[f(a=True, b=False, c=None)]<|tool_call_end|>"),
            json!({"a": true, "b": false, "c": null})
        );
    }

    #[test]
    fn extract_structure() {
        let h = Lfm2Handler;
        let args = |s| h.extract_tool_calls(s).unwrap()[0].arguments.clone();

        // commas and `=` inside a quoted string are not separators
        assert_eq!(
            args("<|tool_call_start|>[say(text=\"a, b = c\")]<|tool_call_end|>"),
            json!({"text": "a, b = c"})
        );
        // empty args; missing end token tolerated
        assert_eq!(
            args("<|tool_call_start|>[get_time()]<|tool_call_end|>"),
            json!({})
        );
        assert_eq!(args("<|tool_call_start|>[f(x=1)]"), json!({"x": 1}));

        // multiple calls in one block
        let calls = h
            .extract_tool_calls("<|tool_call_start|>[one(x=1), two(y=\"hi\")]<|tool_call_end|>")
            .unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1].name, "two");
        assert_eq!(calls[1].arguments, json!({"y": "hi"}));

        // non-call input yields None
        assert!(h.extract_tool_calls("just some text").is_none());
    }

    #[test]
    fn grammar_builds() {
        let h = Lfm2Handler;
        let make = |schema| Tool {
            name: "get_weather".to_string(),
            description: "Get weather".to_string(),
            json_schema: schema,
            function: std::sync::Arc::new(|_| String::new()),
        };

        let g = h
            .generate_grammar(&[make(json!({
                "type": "object",
                "properties": {
                    "city": {"type": "string"},
                    "units": {"type": "string", "enum": ["celsius", "fahrenheit"]},
                    "verbose": {"type": "boolean"}
                },
                "required": ["city"]
            }))])
            .unwrap();
        let s = g.as_str();
        assert!(s.contains("<|tool_call_start|>") && s.contains("<|tool_call_end|>"));
        assert!(s.contains("get_weather("));
        assert!(s.contains("city=") && s.contains("units=") && s.contains("verbose="));

        // empty-params schema still builds
        let g = h
            .generate_grammar(&[make(json!({"type": "object", "properties": {}}))])
            .unwrap();
        assert!(g.as_str().contains("get_weather("));
    }
}
