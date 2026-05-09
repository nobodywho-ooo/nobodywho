use super::{Tool, ToolCall, ToolFormatError, ToolFormatHandler};
use gbnf::builder::{nt, seq, t, GrammarBuilder};
use gbnf::json::json_schema_to_grammar;
use gbnf::GbnfGrammar;
use serde_json::json;
use tracing::{debug, warn};

#[derive(Debug, Clone, Copy)]
pub struct Llama32Handler;

const BEGIN: &str = "<|python_tag|>";
const END: &str = "<|eot_id|>";

#[derive(serde::Deserialize)]
struct LlamaToolCall {
    #[serde(alias = "function")]
    name: String,
    parameters: serde_json::Value,
}

impl ToolFormatHandler for Llama32Handler {
    fn begin_token(&self) -> &str {
        BEGIN
    }

    fn end_token(&self) -> &str {
        END
    }

    fn generate_grammar(&self, tools: &[Tool]) -> Result<GbnfGrammar, ToolFormatError> {
        let tool_call_schemas: serde_json::Value = tools
            .iter()
            .map(|tool| {
                json!(
                    {
                        "type": "object",
                        "properties": {
                            "name": { "const": tool.name },
                            "parameters": tool.json_schema
                        },
                        "required": ["name", "parameters"]
                    }
                )
            })
            .collect();

        let tool_call_schema = json!({ "oneOf": tool_call_schemas });
        let json_grammar = json_schema_to_grammar(tool_call_schema, "root")?;

        let grammar = GrammarBuilder::from_existing(json_grammar)
            .rule(
                "toolcall",
                seq(&[t(BEGIN), nt("ws"), nt("root"), nt("ws")]),
            )
            .rule("superroot", nt("toolcall"))
            .root("superroot")
            .build();

        Ok(grammar)
    }

    fn extract_tool_calls(&self, input: &str) -> Option<Vec<ToolCall>> {
        let mut s = input.trim();
        if let Some(rest) = s.strip_prefix(BEGIN) {
            s = rest.trim_start();
        }
        if let Some(prefix) = s.strip_suffix(END) {
            s = prefix.trim_end();
        }

        let mut stream = serde_json::Deserializer::from_str(s).into_iter::<LlamaToolCall>();
        let first = stream.next()?;

        match first {
            Ok(call) => {
                if stream.next().is_some() {
                    warn!(
                        "Llama-3.x emitted >1 tool call in one assistant turn; \
                         only the first is dispatched (chat template constraint)"
                    );
                }
                debug!(tool_name = %call.name, "Successfully parsed Llama tool call");
                let arguments = repair_string_encoded_structures(call.parameters);
                Some(vec![ToolCall {
                    name: call.name,
                    arguments,
                }])
            }
            Err(e) => {
                debug!(error = %e, json = s, "Failed to parse Llama tool call JSON");
                None
            }
        }
    }
}

/// Llama-3.x chat templates emit `<|python_tag|>` only on the first tool-call turn;
/// post-tool-response turns omit the prefix, so the lazy grammar (keyed on that
/// prefix) never re-fires and the sampler runs unconstrained. Empirically the 1B
/// and 3B variants then emit complex parameters as JSON-encoded strings instead of
/// structured values: `{"set1": "[1,2,3]"}` instead of `{"set1": [1,2,3]}`. This
/// post-process walks the parsed parameters and, when a `Value::String` round-trips
/// through `serde_json::from_str` to an `Array` or `Object`, replaces the string
/// with the parsed structure. Scalars (Number, Bool, Null) are deliberately NOT
/// re-parsed: coercing `"42"` to `42` would silently break tools whose parameter is
/// genuinely `str`.
fn repair_string_encoded_structures(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let repaired = map
                .into_iter()
                .map(|(k, v)| (k, repair_string_encoded_structures(v)))
                .collect();
            serde_json::Value::Object(repaired)
        }
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items.into_iter().map(repair_string_encoded_structures).collect(),
        ),
        serde_json::Value::String(ref s) => {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
                if matches!(
                    parsed,
                    serde_json::Value::Array(_) | serde_json::Value::Object(_)
                ) {
                    debug!(
                        original = %s,
                        "Repaired string-encoded structure into structured JSON"
                    );
                    return repair_string_encoded_structures(parsed);
                }
            }
            value
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn handler() -> Llama32Handler {
        Llama32Handler
    }

    #[test]
    fn p1_round4_with_python_tag_name_field() {
        let input = r#"<|python_tag|>{"name": "circle_area", "parameters": {"radius": "5"}}"#;
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "circle_area");
        assert_eq!(calls[0].arguments, json!({"radius": "5"}));
    }

    #[test]
    fn p2_round4_no_prefix_name_field() {
        let input = r#"{"name": "circle_area", "parameters": {"radius": "12"}}"#;
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(calls[0].name, "circle_area");
        assert_eq!(calls[0].arguments, json!({"radius": "12"}));
    }

    #[test]
    fn p3_round4_function_field_alias() {
        let input = r#"{"function": "get_weather", "parameters": {"city": "Cairo"}}"#;
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(calls[0].name, "get_weather");
        assert_eq!(calls[0].arguments, json!({"city": "Cairo"}));
    }

    #[test]
    fn p4_round4_function_field_alias() {
        let input = r#"{"function": "get_weather", "parameters": {"city": "London"}}"#;
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(calls[0].name, "get_weather");
        assert_eq!(calls[0].arguments, json!({"city": "London"}));
    }

    #[test]
    fn p5_round4_function_field_alias() {
        let input = r#"{"function": "get_weather", "parameters": {"city": "Tokyo"}}"#;
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(calls[0].name, "get_weather");
        assert_eq!(calls[0].arguments, json!({"city": "Tokyo"}));
    }

    #[test]
    fn trailing_eot_is_stripped() {
        let input = r#"<|python_tag|>{"name":"a","parameters":{}}<|eot_id|>"#;
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(calls[0].name, "a");
    }

    #[test]
    fn whitespace_between_prefix_and_json_tolerated() {
        let input = "<|python_tag|>  \n  {\"name\": \"circle_area\", \"parameters\": {\"radius\": \"5\"}}";
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(calls[0].name, "circle_area");
    }

    #[test]
    fn plain_assistant_text_with_inline_json_returns_none() {
        let input = r#"Sure, here is some JSON: {"x": 1}<|eot_id|>"#;
        assert!(handler().extract_tool_calls(input).is_none());
    }

    #[test]
    fn json_without_name_or_function_returns_none() {
        let input = r#"{"x": 1, "parameters": {}}<|eot_id|>"#;
        assert!(handler().extract_tool_calls(input).is_none());
    }

    #[test]
    fn json_without_parameters_returns_none() {
        let input = r#"{"name": "x"}<|eot_id|>"#;
        assert!(handler().extract_tool_calls(input).is_none());
    }

    #[test]
    fn truncated_mid_json_returns_none() {
        let input = r#"{"name": "x", "parameters": {"a""#;
        assert!(handler().extract_tool_calls(input).is_none());
    }

    #[test]
    fn two_blocks_returns_only_first() {
        let input = r#"{"name":"a","parameters":{}}{"name":"b","parameters":{}}"#;
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(calls.len(), 1, "single-call constraint enforced");
        assert_eq!(calls[0].name, "a");
    }

    #[test]
    fn empty_input_returns_none() {
        assert!(handler().extract_tool_calls("").is_none());
    }

    #[test]
    fn tokens_match_chat_template() {
        let h = handler();
        assert_eq!(h.begin_token(), "<|python_tag|>");
        assert_eq!(h.end_token(), "<|eot_id|>");
    }

    #[test]
    fn grammar_contains_ws_rule() {
        let tool = Tool::new(
            "circle_area",
            "Compute area of circle",
            json!({
                "type": "object",
                "properties": { "radius": { "type": "number" } },
                "required": ["radius"]
            }),
            std::sync::Arc::new(|_| "78.54".to_string()),
        );
        let grammar = handler().generate_grammar(&[tool]).unwrap();
        let s = grammar.as_str();
        assert!(s.contains("ws ::="), "expected ws rule in:\n{}", s);
    }

    #[test]
    fn grammar_constrains_to_known_tools() {
        let tool = Tool::new(
            "circle_area",
            "Compute area of circle",
            json!({
                "type": "object",
                "properties": { "radius": { "type": "number" } },
                "required": ["radius"]
            }),
            std::sync::Arc::new(|_| "78.54".to_string()),
        );
        let grammar = handler().generate_grammar(&[tool]).unwrap();
        let s = grammar.as_str();
        assert!(
            s.contains("circle_area"),
            "expected tool name as const literal in grammar:\n{}",
            s
        );
        assert!(
            s.contains("<|python_tag|>"),
            "expected begin token literal in grammar:\n{}",
            s
        );
    }

    #[test]
    fn extract_repairs_string_encoded_array_argument() {
        // Direct reproduction of the empirical Llama-3.2-3B failure on
        // test_tool_with_tuple: post-tool-response turns omit `<|python_tag|>`
        // so the lazy grammar never re-fires; the sampler runs unconstrained and
        // the model emits `string_int_pair` as a JSON-encoded string instead of
        // an array. The handler recovers by walking parameters and re-parsing
        // any String that round-trips to an Array or Object.
        let input = r#"{"name":"multiply_strings","parameters":{"string_int_pair":"[\"BingBong\", 3]"}}"#;
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(calls[0].name, "multiply_strings");
        assert_eq!(
            calls[0].arguments,
            json!({"string_int_pair": ["BingBong", 3]}),
            "string-encoded array must be repaired into a real array"
        );
    }

    #[test]
    fn extract_repairs_string_encoded_object_argument() {
        let input = r#"{"name":"calculate_volume","parameters":{"dimensions":"{\"width\":30,\"height\":20,\"depth\":10}"}}"#;
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(
            calls[0].arguments,
            json!({"dimensions": {"width": 30, "height": 20, "depth": 10}}),
        );
    }

    #[test]
    fn extract_does_not_coerce_legitimate_string_args() {
        // Critical: the repair must NOT touch genuine string parameters even when
        // they happen to round-trip through serde_json::from_str as a scalar.
        // "42" parses as a number; coercing it would break tools whose parameter
        // is actually `text: str`. Only Array/Object outputs should be repaired.
        let input = r#"{"name":"sparklify","parameters":{"text":"42"}}"#;
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(
            calls[0].arguments,
            json!({"text": "42"}),
            "scalar string args must remain strings even when JSON-parseable as number"
        );

        let input = r#"{"name":"sparklify","parameters":{"text":"true"}}"#;
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(calls[0].arguments, json!({"text": "true"}));

        let input = r#"{"name":"sparklify","parameters":{"text":"julemand"}}"#;
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(calls[0].arguments, json!({"text": "julemand"}));
    }

    #[test]
    fn extract_repairs_recursively_nested_string_encoding() {
        // Some small variants double-encode. Walk the value tree.
        let input = r#"{"name":"add_list_of_vectors","parameters":{"list_of_vectors":"[[1,2,3],[4,5,6]]"}}"#;
        let calls = handler().extract_tool_calls(input).unwrap();
        assert_eq!(
            calls[0].arguments,
            json!({"list_of_vectors": [[1, 2, 3], [4, 5, 6]]}),
        );
    }
}
