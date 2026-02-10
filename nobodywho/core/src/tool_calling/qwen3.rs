use super::grammar_builder::{nt, nt_plus, t, t_star, GrammarBuilder};
use super::{Tool, ToolCall, ToolFormatError, ToolFormatHandler};
use serde_json::json;
use tracing::debug;

#[derive(Debug, Clone, Copy)]
pub struct Qwen3Handler;

impl ToolFormatHandler for Qwen3Handler {
    fn begin_token(&self) -> &str {
        "<tool_call>"
    }

    fn end_token(&self) -> &str {
        "</tool_call>"
    }

    fn generate_grammar(&self, tools: &[Tool]) -> Result<gbnf::Grammar, ToolFormatError> {
        let tool_call_schemas: serde_json::Value = tools
            .iter()
            .map(|tool| {
                json!(
                    {
                        "type": "object",
                        "properties": {
                            "name": { "const": tool.name, },
                            "arguments": tool.json_schema
                        },
                        "required": ["name", "arguments"]
                    }
                )
            })
            .collect();

        let tool_call_schema = json!(
            { "oneOf": tool_call_schemas }
        );

        // Generate JSON grammar from schema, then extend it with wrapping rules
        let json_grammar = gbnf::Grammar::from_json_schema(&tool_call_schema.to_string())?;

        let grammar = GrammarBuilder::from_existing(json_grammar)
            .rule("ws", vec![t_star(" ")])
            .rule(
                "toolcall",
                vec![
                    t(self.begin_token()),
                    nt("ws"),
                    nt("root"),
                    nt("ws"),
                    t(self.end_token()),
                    nt("ws"),
                ],
            )
            .rule("superroot", vec![nt_plus("toolcall")])
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
                        debug!(tool_name = %tool_call.name, "Successfully parsed tool call");
                        Some(tool_call)
                    }
                    Err(e) => {
                        debug!(error = %e, json = json_str, "Failed to parse tool call JSON");
                        None
                    }
                }
            })
            .collect();

        if !tool_calls.is_empty() {
            Some(tool_calls)
        } else {
            debug!("No tool calls detected in message");
            None
        }
    }

    fn serialize_tool(&self, tool: &Tool) -> serde_json::Value {
        json!({
            "type": "function",
            "function": {
                "name": &tool.name,
                "description": &tool.description,
                "parameters": &tool.json_schema,
            }
        })
    }

    fn serialize_tool_call(&self, tool_call: &ToolCall) -> serde_json::Value {
        json!({
            "name": &tool_call.name,
            "arguments": &tool_call.arguments,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_qwen3_extract_single_tool_call() {
        let handler = Qwen3Handler;
        let input = r#"<tool_call>{"name": "get_weather", "arguments": {"location": "San Francisco"}}</tool_call>"#;

        let result = handler.extract_tool_calls(input);
        assert!(result.is_some());

        let tool_calls = result.unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "get_weather");
        assert_eq!(
            tool_calls[0].arguments,
            json!({"location": "San Francisco"})
        );
    }

    #[test]
    fn test_qwen3_extract_multiple_tool_calls() {
        let handler = Qwen3Handler;
        let input = r#"<tool_call>{"name": "tool1", "arguments": {"a": 1}}</tool_call><tool_call>{"name": "tool2", "arguments": {"b": 2}}</tool_call>"#;

        let result = handler.extract_tool_calls(input);
        assert!(result.is_some());

        let tool_calls = result.unwrap();
        assert_eq!(tool_calls.len(), 2);
        assert_eq!(tool_calls[0].name, "tool1");
        assert_eq!(tool_calls[1].name, "tool2");
    }

    #[test]
    fn test_qwen3_extract_no_tool_calls() {
        let handler = Qwen3Handler;
        let input = "This is just regular text without any tool calls.";

        let result = handler.extract_tool_calls(input);
        assert!(result.is_none());
    }
}
