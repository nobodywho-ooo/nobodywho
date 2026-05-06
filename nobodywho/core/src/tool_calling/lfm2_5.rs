use super::{Tool, ToolCall, ToolFormatError, ToolFormatHandler};
use gbnf::GbnfGrammar;
use serde_json::{json, Value};
use tracing::{debug, warn};

#[derive(Debug, Clone, Copy)]
pub struct Lfm2_5Handler;

// LFM2.5 family (LFM2.5-350M, LFM2.5-1.2B-Instruct, LFM2.5-1.2B-Tool, ...) writes
// PYTHONIC tool calls between `<|tool_call_start|>` and `<|tool_call_end|>` tokens
// per the LFM2.5 model card example:
//
//   <|tool_call_start|>[get_candidate_status(candidate_id="12345")]<|tool_call_end|>
//
// The list may contain multiple calls separated by `,`. Args are kwargs
// (`key=value`) with values: string ("..." or '...'), int, float, bool, null.
//
// This handler runs in PARSER-ONLY mode: `generate_grammar` returns Err so the
// engine sets `tool_grammar=None` (chat.rs:1240-1265 verified). Sampler runs
// unconstrained; `extract_tool_calls` parses the wire format from the response.
// The mirror of llama.cpp's pre-autoparser working configuration for LFM2.5
// (see llama.cpp issue #20245).

const BEGIN: &str = "<|tool_call_start|>";
const END: &str = "<|tool_call_end|>";

impl ToolFormatHandler for Lfm2_5Handler {
    fn begin_token(&self) -> &str {
        BEGIN
    }

    fn end_token(&self) -> &str {
        END
    }

    fn generate_grammar(&self, _tools: &[Tool]) -> Result<GbnfGrammar, ToolFormatError> {
        // INTENTIONAL Err — engine path (chat.rs:1240-1265) keeps
        // tool_format=Some(handler) and sets tool_grammar=None on Err.
        // Sampler runs unconstrained; the model emits its trained Pythonic
        // wire format; extract_tool_calls parses it on the way out.
        Err(ToolFormatError::UnsupportedFormat(
            "LFM2.5 handler runs in parser-only mode; no GBNF constraint".to_string(),
        ))
    }

    fn extract_tool_calls(&self, input: &str) -> Option<Vec<ToolCall>> {
        // Per LFM2.5 model card the assistant turn is:
        //   <|tool_call_start|>[fn1(...), fn2(...)]<|tool_call_end|>{trailing NL}
        // Locate the FIRST wrap; tail text after END is ignored. If begin_token
        // never appears, this is plain NL — return None and the engine loop
        // exits naturally with the model's plain-text response.
        let begin_at = input.find(BEGIN)?;
        let after_begin = &input[begin_at + BEGIN.len()..];
        let end_at = after_begin.find(END)?;
        let inner = after_begin[..end_at].trim();

        // Inner is `[fn1(...), fn2(...)]` Pythonic list.
        let bracketed = inner.strip_prefix('[').and_then(|x| x.strip_suffix(']'))?;
        let body = bracketed.trim();
        if body.is_empty() {
            debug!("LFM2.5 emitted empty tool call list");
            return None;
        }

        let raw_calls = split_top_level_calls(body);
        if raw_calls.is_empty() {
            debug!(input = %input, "LFM2.5 tool call list parsing failed (no calls)");
            return None;
        }

        let mut out = Vec::with_capacity(raw_calls.len());
        for raw in raw_calls {
            match parse_pythonic_call(raw.trim()) {
                Some(tc) => out.push(tc),
                None => {
                    debug!(call = %raw, "LFM2.5 single-call parse failed");
                    return None;
                }
            }
        }

        if out.len() > 1 {
            warn!(
                count = out.len(),
                "LFM2.5 emitted >1 tool call in one assistant turn"
            );
        }
        Some(out)
    }
}

// ----- Pythonic call parsing helpers (private) -----

/// Split `fn1(...), fn2(...), ...` (or `k=v, k=v` arg lists) into
/// individual top-level chunks, respecting `(` / `)` nesting and string
/// quoting so commas inside args / strings are not separators.
fn split_top_level_calls(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut depth: i32 = 0;
    let mut start: usize = 0;
    let mut in_str: Option<u8> = None;
    let mut out: Vec<&str> = Vec::new();
    for (i, &b) in bytes.iter().enumerate() {
        match (in_str, b) {
            (Some(q), c) if c == q => in_str = None,
            (Some(_), _) => {}
            (None, b'"') => in_str = Some(b'"'),
            (None, b'\'') => in_str = Some(b'\''),
            (None, b'(') => depth += 1,
            (None, b')') => depth -= 1,
            (None, b',') if depth == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

fn parse_pythonic_call(s: &str) -> Option<ToolCall> {
    let open = s.find('(')?;
    let name = s[..open].trim().to_string();
    if name.is_empty() {
        return None;
    }
    let close = s.rfind(')')?;
    if close <= open {
        return None;
    }
    let args_src = s[open + 1..close].trim();

    let mut args_obj = serde_json::Map::new();
    if !args_src.is_empty() {
        for kv in split_top_level_calls(args_src) {
            let kv = kv.trim();
            if kv.is_empty() {
                continue;
            }
            let eq = kv.find('=')?;
            let key = kv[..eq].trim().to_string();
            let raw_val = kv[eq + 1..].trim();
            let v = parse_pythonic_value(raw_val)?;
            args_obj.insert(key, v);
        }
    }
    Some(ToolCall {
        name,
        arguments: Value::Object(args_obj),
    })
}

fn parse_pythonic_value(s: &str) -> Option<Value> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        if s.len() < 2 {
            return None;
        }
        return Some(Value::String(s[1..s.len() - 1].to_string()));
    }
    match s {
        "true" | "True" => return Some(Value::Bool(true)),
        "false" | "False" => return Some(Value::Bool(false)),
        "null" | "None" => return Some(Value::Null),
        _ => {}
    }
    if let Ok(i) = s.parse::<i64>() {
        return Some(json!(i));
    }
    if let Ok(f) = s.parse::<f64>() {
        return Some(json!(f));
    }
    Some(Value::String(s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn handler() -> Lfm2_5Handler {
        Lfm2_5Handler
    }

    fn dummy_tool(name: &str, prop: &str) -> Tool {
        Tool::new(
            name,
            "test tool",
            json!({
                "type": "object",
                "properties": { prop: { "type": "number" } },
                "required": [prop]
            }),
            Arc::new(|_| "ok".to_string()),
        )
    }

    #[test]
    fn parse_single_kwarg_string() {
        let input = r#"<|tool_call_start|>[get_weather(city="Cairo")]<|tool_call_end|>"#;
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "get_weather");
        assert_eq!(calls[0].arguments, json!({"city": "Cairo"}));
    }

    #[test]
    fn parse_single_kwarg_number() {
        let input = r#"<|tool_call_start|>[circle_area(radius=12.5)]<|tool_call_end|>"#;
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(calls[0].name, "circle_area");
        assert_eq!(calls[0].arguments, json!({"radius": 12.5}));
    }

    #[test]
    fn parse_multi_call() {
        let input =
            r#"<|tool_call_start|>[get_weather(city="London"), get_weather(city="Tokyo")]<|tool_call_end|>"#;
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].arguments, json!({"city": "London"}));
        assert_eq!(calls[1].arguments, json!({"city": "Tokyo"}));
    }

    #[test]
    fn parse_with_single_quotes() {
        let input = r#"<|tool_call_start|>[get_weather(city='Cairo')]<|tool_call_end|>"#;
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(calls[0].arguments, json!({"city": "Cairo"}));
    }

    #[test]
    fn parse_negative_int() {
        let input = r#"<|tool_call_start|>[move(steps=-3)]<|tool_call_end|>"#;
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(calls[0].arguments, json!({"steps": -3}));
    }

    #[test]
    fn parse_bool_kwarg() {
        let input = r#"<|tool_call_start|>[set_flag(active=true, dry=False)]<|tool_call_end|>"#;
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(calls[0].arguments, json!({"active": true, "dry": false}));
    }

    #[test]
    fn no_args() {
        let input = r#"<|tool_call_start|>[noop()]<|tool_call_end|>"#;
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(calls[0].name, "noop");
        assert_eq!(calls[0].arguments, json!({}));
    }

    #[test]
    fn empty_list_returns_none() {
        let input = "<|tool_call_start|>[]<|tool_call_end|>";
        assert!(handler().extract_tool_calls(input).is_none());
    }

    #[test]
    fn malformed_returns_none() {
        let input = "<|tool_call_start|>not python at all<|tool_call_end|>";
        assert!(handler().extract_tool_calls(input).is_none());
    }

    #[test]
    fn no_wrap_returns_none() {
        let input = r#"I'll help you with that."#;
        assert!(handler().extract_tool_calls(input).is_none());
    }

    #[test]
    fn double_wrap_emit_extracts_first_call() {
        // LFM2.5-1.2B-Instruct empirically emits a double-wrap on multi-turn weather:
        //   <|tool_call_start|>[get_weather(city="Cairo")]<|tool_call_end|>[get_weather(city="Cairo")]<|tool_call_end|>
        // Parser should still extract the first valid call (between begin and FIRST end token).
        let input = concat!(
            "<|tool_call_start|>[get_weather(city=\"Cairo\")]<|tool_call_end|>",
            "[get_weather(city=\"Cairo\")]<|tool_call_end|>"
        );
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(calls.len(), 1, "should extract first call only, ignore tail");
        assert_eq!(calls[0].name, "get_weather");
        assert_eq!(calls[0].arguments, json!({"city": "Cairo"}));
    }

    #[test]
    fn tail_text_after_end_token_is_ignored() {
        // Per LFM2.5 model card: assistant turn can be
        //   <|tool_call_start|>[fn(...)]<|tool_call_end|>{NL summary}
        let input = concat!(
            "<|tool_call_start|>[circle_area(radius=5)]<|tool_call_end|>",
            "Checking the area now."
        );
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "circle_area");
        assert_eq!(calls[0].arguments, json!({"radius": 5}));
    }

    #[test]
    fn nl_preamble_before_begin_token_is_ignored() {
        let input = concat!(
            "I'll fetch that for you.\n",
            "<|tool_call_start|>[get_weather(city=\"Cairo\")]<|tool_call_end|>"
        );
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(calls[0].arguments, json!({"city": "Cairo"}));
    }

    #[test]
    fn tokens_match_chat_template() {
        let h = handler();
        assert_eq!(h.begin_token(), "<|tool_call_start|>");
        assert_eq!(h.end_token(), "<|tool_call_end|>");
    }

    #[test]
    fn generate_grammar_returns_err_intentionally() {
        // Plan: parser-only mode. Engine reads Err and sets tool_grammar=None
        // while keeping tool_format=Some(handler).
        let r = handler().generate_grammar(&[dummy_tool("circle_area", "radius")]);
        assert!(r.is_err(), "generate_grammar must return Err for parser-only handler");
    }

    #[test]
    fn generate_grammar_returns_err_for_empty_tools() {
        let r = handler().generate_grammar(&[]);
        assert!(r.is_err());
    }

    #[test]
    fn split_top_level_respects_parens() {
        let parts = split_top_level_calls("a(1,2), b(3, c(4,5)), d");
        assert_eq!(parts, vec!["a(1,2)", " b(3, c(4,5))", " d"]);
    }

    #[test]
    fn split_top_level_respects_quotes() {
        let parts = split_top_level_calls(r#"city="Cai,ro", n=1"#);
        assert_eq!(parts, vec![r#"city="Cai,ro""#, " n=1"]);
    }
}
