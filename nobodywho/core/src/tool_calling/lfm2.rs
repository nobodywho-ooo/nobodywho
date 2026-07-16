//! LFM2 tool calling. The model emits a Python-style call list
//! `<|tool_call_start|>[get_weather(location="Paris")]<|tool_call_end|>` — calls
//! separated by `, `, args as `key=value` (strings double-quoted, everything else
//! JSON-ish). We grammar-constrain to that shape and parse values leniently
//! (also tolerating Python `True`/`False`/`None`).

use super::{Tool, ToolCall, ToolFormatError, ToolFormatHandler};
use serde_json::{Map, Value};
use std::collections::HashSet;
use tracing::debug;

const BEGIN_TOKEN: &str = "<|tool_call_start|>";
const END_TOKEN: &str = "<|tool_call_end|>";

#[derive(Debug, Clone, Copy)]
pub struct Lfm2Handler;

impl ToolFormatHandler for Lfm2Handler {
    fn begin_token(&self) -> &str {
        BEGIN_TOKEN
    }

    fn end_token(&self) -> &str {
        END_TOKEN
    }

    fn to_lark(&self, tools: &[Tool]) -> Result<String, ToolFormatError> {
        let mut lark = String::from("%llguidance {}\n");

        let mut tool_rule_names = Vec::with_capacity(tools.len());
        for (ti, tool) in tools.iter().enumerate() {
            let tool_rule = lark_tool_rule(tool, ti, &mut lark)?;
            tool_rule_names.push(tool_rule);
        }

        let tool_alt_str = tool_rule_names.join(" | ");
        lark.push_str(&format!("lfm2_tool_alt: {tool_alt_str}\n"));
        lark.push_str("lfm2_calllist: lfm2_tool_alt | lfm2_tool_alt \", \" lfm2_calllist\n");
        lark.push_str(&format!(
            "start: \"{BEGIN_TOKEN}\" \"[\" lfm2_calllist \"]\" \"{END_TOKEN}\"\n"
        ));

        Ok(lark)
    }

    /// Vocabulary hint: JSON string-value body bytes (excludes `"`, `\`, control chars).
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

fn required_params(tool: &Tool) -> HashSet<&str> {
    tool.json_schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default()
}

/// Map any non-alphanumeric character to `_` for use in Lark rule names.
fn sanitize_lark(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// Build the `from-i`/`more-i` rule families for ordered optional params.
///
/// - `from-i`: nothing emitted yet; the first emitted param has no leading comma.
/// - `more-i`: a param was already emitted; every further param gets `", "`.
///
/// Returns the entry rule name (`{tprefix}_from_0`).
fn lark_arglist_rules(tprefix: &str, params: &[(String, bool)], lark: &mut String) -> String {
    let n = params.len();

    // Base cases: nothing left to emit.
    lark.push_str(&format!("{tprefix}_from_{n}: \"\"\n"));
    lark.push_str(&format!("{tprefix}_more_{n}: \"\"\n"));

    for i in (0..n).rev() {
        let (kv_rule, required) = &params[i];
        let from_i = format!("{tprefix}_from_{i}");
        let more_i = format!("{tprefix}_more_{i}");
        let from_next = format!("{tprefix}_from_{}", i + 1);
        let more_next = format!("{tprefix}_more_{}", i + 1);

        if *required {
            // from-i  ::= kv more-(i+1)
            // more-i  ::= ", " kv more-(i+1)
            lark.push_str(&format!("{from_i}: {kv_rule} {more_next}\n"));
            lark.push_str(&format!("{more_i}: \", \" {kv_rule} {more_next}\n"));
        } else {
            // from-i  ::= (kv more-(i+1)) | from-(i+1)
            // more-i  ::= (", " kv more-(i+1)) | more-(i+1)
            lark.push_str(&format!(
                "{from_i}: ({kv_rule} {more_next}) | {from_next}\n"
            ));
            lark.push_str(&format!(
                "{more_i}: (\", \" {kv_rule} {more_next}) | {more_next}\n"
            ));
        }
    }

    format!("{tprefix}_from_0")
}

fn lark_tool_rule(
    tool: &Tool,
    tool_index: usize,
    lark: &mut String,
) -> Result<String, ToolFormatError> {
    let tprefix = format!("lfm2_t{tool_index}");
    let required = required_params(tool);

    // (kv-rule-name, is-required) per property, preserving declaration order.
    let mut params: Vec<(String, bool)> = Vec::new();
    if let Some(props) = tool
        .json_schema
        .get("properties")
        .and_then(|p| p.as_object())
    {
        for (pi, (pname, pschema)) in props.iter().enumerate() {
            let pprefix = format!("{tprefix}_p{pi}_{}", sanitize_lark(pname));

            let val_rule = format!("{pprefix}_val");
            let schema_str = serde_json::to_string(pschema)
                .map_err(|e| ToolFormatError::GrammarGenerationFailed(e.to_string()))?;
            lark.push_str(&format!("{val_rule}: %json {schema_str}\n"));

            let kv_rule = format!("{pprefix}_kv");
            let pname_esc = super::escape_lark_string(pname);
            lark.push_str(&format!("{kv_rule}: \"{pname_esc}=\" {val_rule}\n"));

            params.push((kv_rule, required.contains(pname.as_str())));
        }
    }

    let arglist_rule = lark_arglist_rules(&tprefix, &params, lark);

    let tool_rule = format!("{tprefix}_call");
    lark.push_str(&format!(
        "{tool_rule}: \"{}(\" {arglist_rule} \")\"\n",
        super::escape_lark_string(&tool.name)
    ));

    Ok(tool_rule)
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
    fn lark_builds() {
        let h = Lfm2Handler;
        let make = |schema| Tool {
            name: "get_weather".to_string(),
            description: "Get weather".to_string(),
            json_schema: schema,
            function: std::sync::Arc::new(|_| String::new()),
        };

        let s = h
            .to_lark(&[make(json!({
                "type": "object",
                "properties": {
                    "city": {"type": "string"},
                    "units": {"type": "string", "enum": ["celsius", "fahrenheit"]},
                    "verbose": {"type": "boolean"}
                },
                "required": ["city"]
            }))])
            .unwrap();
        assert!(s.contains("%llguidance"));
        assert!(s.contains("<|tool_call_start|>") && s.contains("<|tool_call_end|>"));
        assert!(s.contains("get_weather("));
        assert!(s.contains("city=") && s.contains("units=") && s.contains("verbose="));

        // empty-params schema still builds
        let s = h
            .to_lark(&[make(json!({"type": "object", "properties": {}}))])
            .unwrap();
        assert!(s.contains("get_weather("));
    }
}
