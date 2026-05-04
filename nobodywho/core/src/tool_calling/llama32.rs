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
                Some(vec![ToolCall {
                    name: call.name,
                    arguments: call.parameters,
                }])
            }
            Err(e) => {
                debug!(error = %e, json = s, "Failed to parse Llama tool call JSON");
                None
            }
        }
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
}
