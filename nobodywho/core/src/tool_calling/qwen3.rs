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

    fn to_lark(
        &self,
        tools: &[Tool],
        model: Option<&llama_cpp_2::model::LlamaModel>,
    ) -> Result<String, ToolFormatError> {
        let tool_schemas: Vec<serde_json::Value> = tools
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

        let schema_str = serde_json::to_string(&json!({ "oneOf": tool_schemas }))
            .map_err(|e| ToolFormatError::GrammarGenerationFailed(e.to_string()))?;

        let mut lark = String::from("%llguidance {}\n");
        lark.push_str("start: toolcall+\n");
        let begin = super::lark_delimiter(model, "<tool_call>");
        let end = super::lark_delimiter(model, "</tool_call>");
        lark.push_str(&format!("toolcall: {begin} ws? body ws? {end} ws?\n"));
        lark.push_str(&format!("body: %json {schema_str}\n"));
        lark.push_str("ws: /[ \\t\\n\\r]+/\n");
        Ok(lark)
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
