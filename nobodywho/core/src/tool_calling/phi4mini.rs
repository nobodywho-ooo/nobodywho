use gbnf::builder::{nt, seq, t, GrammarBuilder};
use super::{Tool, ToolCall, ToolFormatError, ToolFormatHandler};
use gbnf::json::json_schema_to_grammar;
use gbnf::GbnfGrammar;
use serde_json::json;
use tracing::debug;

#[derive(Debug, Clone, Copy)]
pub struct Phi4MiniHandler;

impl ToolFormatHandler for Phi4MiniHandler {
    fn begin_token(&self) -> &str {
        "<|tool_call|>"
    }

    fn end_token(&self) -> &str {
        "<|/tool_call|>"
    }

    fn uses_template_for_tools(&self) -> bool {
        true
    }

    fn generate_grammar(&self, tools: &[Tool]) -> Result<GbnfGrammar, ToolFormatError> {
        let tool_call_schemas: serde_json::Value = tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "object",
                    "properties": {
                        "name": { "const": tool.name },
                        "arguments": tool.json_schema
                    },
                    "required": ["name", "arguments"]
                })
            })
            .collect();

        let tool_call_schema = json!({ "oneOf": tool_call_schemas });

        let json_grammar = json_schema_to_grammar(tool_call_schema, "root")?;

        let grammar = GrammarBuilder::from_existing(json_grammar)
            .rule(
                "toolcall",
                seq(&[
                    t(self.begin_token()),
                    nt("ws"),
                    nt("root"),
                    nt("ws"),
                    t(self.end_token()),
                    nt("ws"),
                ]),
            )
            .rule("superroot", nt("toolcall"))
            .root("superroot")
            .build();

        Ok(grammar)
    }

    fn extract_tool_calls(&self, input: &str) -> Option<Vec<ToolCall>> {
        let pattern = format!(
            r"{}([\s\S]*?){}",
            regex::escape(self.begin_token()),
            regex::escape(self.end_token())
        );
        let re = regex::Regex::new(&pattern).expect("Invalid regex");

        let tool_calls: Vec<ToolCall> = re
            .captures_iter(input)
            .filter_map(|cap| {
                let json_str = cap[1].trim();
                match serde_json::from_str::<ToolCall>(json_str) {
                    Ok(tool_call) => {
                        debug!(tool_name = %tool_call.name, "Successfully parsed Phi-4-mini tool call");
                        Some(tool_call)
                    }
                    Err(e) => {
                        debug!(error = %e, json = json_str, "Failed to parse Phi-4-mini tool call JSON");
                        None
                    }
                }
            })
            .collect();

        if !tool_calls.is_empty() {
            Some(tool_calls)
        } else {
            debug!("No Phi-4-mini tool calls detected in message");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    fn make_tool(name: &str) -> Tool {
        Tool::new(
            name,
            "A test tool",
            json!({"type": "object", "properties": {"arg": {"type": "string"}}, "required": ["arg"]}),
            Arc::new(|_| "result".to_string()),
        )
    }

    #[test]
    fn test_begin_end_tokens() {
        let h = Phi4MiniHandler;
        assert_eq!(h.begin_token(), "<|tool_call|>");
        assert_eq!(h.end_token(), "<|/tool_call|>");
    }

    #[test]
    fn test_uses_template_for_tools() {
        assert!(Phi4MiniHandler.uses_template_for_tools());
    }

    #[test]
    fn test_system_message_tool_injection_returns_none() {
        let tools = vec![make_tool("get_weather")];
        assert!(Phi4MiniHandler.system_message_tool_injection(&tools).is_none());
    }

    #[test]
    fn test_extract_single_tool_call() {
        let h = Phi4MiniHandler;
        let input = r#"<|tool_call|>{"name": "get_weather", "arguments": {"location": "Paris"}}<|/tool_call|>"#;

        let result = h.extract_tool_calls(input);
        assert!(result.is_some());
        let calls = result.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "get_weather");
        assert_eq!(calls[0].arguments, json!({"location": "Paris"}));
    }

    #[test]
    fn test_extract_multiple_tool_calls() {
        let h = Phi4MiniHandler;
        let input = r#"<|tool_call|>{"name": "tool1", "arguments": {"a": 1}}<|/tool_call|><|tool_call|>{"name": "tool2", "arguments": {"b": 2}}<|/tool_call|>"#;

        let result = h.extract_tool_calls(input);
        assert!(result.is_some());
        let calls = result.unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "tool1");
        assert_eq!(calls[1].name, "tool2");
    }

    #[test]
    fn test_extract_no_tool_calls() {
        let h = Phi4MiniHandler;
        let input = "This is regular text without any tool calls.";
        assert!(h.extract_tool_calls(input).is_none());
    }
}
