use super::{Tool, ToolCall, ToolFormatError, ToolFormatHandler};
use serde_json::json;
use tracing::debug;

#[derive(Debug, Clone, Copy)]
pub struct FunctionGemmaHandler;

impl ToolFormatHandler for FunctionGemmaHandler {
    fn begin_token(&self) -> &str {
        "<start_function_call>"
    }

    fn end_token(&self) -> &str {
        "<end_function_call>"
    }

    fn to_lark(&self, tools: &[Tool]) -> Result<String, ToolFormatError> {
        let mut lark = String::from("%llguidance {}\n");
        lark.push_str(
            "start: \"<start_function_call>\" ws? functioncall ws? \"<end_function_call>\" ws?\n",
        );

        let alts: Vec<String> = (0..tools.len()).map(|i| format!("tool_{i}")).collect();
        lark.push_str(&format!("functioncall: {}\n", alts.join(" | ")));

        for (i, tool) in tools.iter().enumerate() {
            let properties = tool
                .json_schema
                .get("properties")
                .and_then(|p| p.as_object());
            let name = &tool.name;
            let mut rule = format!("tool_{i}: \"call:{name}{{\"");

            if let Some(props) = properties {
                if !props.is_empty() {
                    let mut first = true;
                    for param_name in props.keys() {
                        if !first {
                            rule.push_str(" \", \"");
                        }
                        rule.push_str(&format!(" \"{param_name}:\" value"));
                        first = false;
                    }
                }
            }

            rule.push_str(" \"}\"");
            lark.push_str(&rule);
            lark.push('\n');
        }

        lark.push_str("value: \"<escape>\" /[^<>{},:]+/ \"<escape>\"\n");
        lark.push_str("ws: / */\n");
        Ok(lark)
    }

    /// Returns a vocabulary hint that speeds up grammar-constrained token selection.
    ///
    /// The regex covers the most common token content in this format: value
    /// bytes that are not structural delimiters (`< > { } , :`). llguidance
    /// pre-computes a bitmask for this pattern at startup; when every valid
    /// token at the current grammar position matches the pattern, it uses the
    /// bitmask directly instead of scanning the full vocabulary.
    fn slice_regexes(&self) -> Vec<String> {
        vec![r"[^<>{},:]+".to_string()]
    }

    fn extract_tool_calls(&self, input: &str) -> Option<Vec<ToolCall>> {
        // Regex to capture the entire FunctionGemma structure:
        // <start_function_call>call:function_name{params}<end_function_call>
        let tool_call_regex =
            regex::Regex::new(r"<start_function_call>\s*call:(\w+)\{(.*?)\}\s*<end_function_call>")
                .expect("Invalid regex");

        // Regex to capture individual parameters: param_name:<escape>value<escape>
        let param_regex = regex::Regex::new(r"(\w+):<escape>(.*?)<escape>").expect("Invalid regex");

        let tool_calls: Vec<ToolCall> = tool_call_regex
            .captures_iter(input)
            .map(|cap| {
                let name = &cap[1];
                let params_str = &cap[2];

                let mut arguments = json!({});
                for param_cap in param_regex.captures_iter(params_str) {
                    let key = &param_cap[1];
                    let value_str = &param_cap[2];

                    let value =
                        serde_json::from_str(value_str).unwrap_or_else(|_| json!(value_str));

                    arguments[key] = value;
                }

                debug!(tool_name = %name, "Successfully parsed FunctionGemma tool call");
                ToolCall {
                    name: name.to_string(),
                    arguments,
                }
            })
            .collect();

        if !tool_calls.is_empty() {
            Some(tool_calls)
        } else {
            debug!("No FunctionGemma tool calls detected in message");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_functiongemma_extract_simple_call() {
        let handler = FunctionGemmaHandler;
        let input = r#"<start_function_call>call:get_weather{location:<escape>San Francisco<escape>}<end_function_call>"#;

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
    fn test_functiongemma_extract_multiple_params() {
        let handler = FunctionGemmaHandler;
        let input = r#"<start_function_call>call:calculate{x:<escape>10<escape>, y:<escape>20<escape>, op:<escape>add<escape>}<end_function_call>"#;

        let result = handler.extract_tool_calls(input);
        assert!(result.is_some());

        let tool_calls = result.unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "calculate");
        // Numbers are parsed as JSON numbers, not strings
        assert_eq!(tool_calls[0].arguments["x"], json!(10));
        assert_eq!(tool_calls[0].arguments["y"], json!(20));
        assert_eq!(tool_calls[0].arguments["op"], json!("add"));
    }

    #[test]
    fn test_functiongemma_extract_no_params() {
        let handler = FunctionGemmaHandler;
        let input = r#"<start_function_call>call:get_time{}<end_function_call>"#;

        let result = handler.extract_tool_calls(input);
        assert!(result.is_some());

        let tool_calls = result.unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "get_time");
        assert_eq!(tool_calls[0].arguments, json!({}));
    }

    #[test]
    fn test_functiongemma_extract_no_tool_calls() {
        let handler = FunctionGemmaHandler;
        let input = "This is just regular text without any tool calls.";

        let result = handler.extract_tool_calls(input);
        assert!(result.is_none());
    }
}
