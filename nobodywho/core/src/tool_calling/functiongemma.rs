use super::grammar_builder::{not_chars, nt, seq, t, t_star, GrammarBuilder};
use super::{Tool, ToolCall, ToolFormatError, ToolFormatHandler};
use gbnf::GbnfGrammar;
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

    fn generate_grammar(&self, tools: &[Tool]) -> Result<GbnfGrammar, ToolFormatError> {
        // FunctionGemma format: call:tool_name{param1:<escape>value<escape>, param2:<escape>value<escape>}
        let mut builder = GrammarBuilder::new().rule("ws", t_star(" ")).rule(
            "value",
            seq(&[
                t("<escape>"),
                not_chars(&['<', '>', '{', '}', ',', ':']),
                t("<escape>"),
            ]),
        );

        let tool_rules: Vec<_> = tools
            .iter()
            .map(|tool| {
                // Sanitize the tool name for GBNF (only alphanumeric allowed, no underscores)
                tool.name
                    .chars()
                    .filter(|c| c.is_alphanumeric())
                    .collect::<String>()
            })
            .collect();

        for (tool_name, tool) in tool_rules.iter().zip(tools.iter()) {
            let properties = tool
                .json_schema
                .get("properties")
                .and_then(|p| p.as_object());

            let mut items = vec![t("call:"), t(&tool.name), t("{")];

            if let Some(props) = properties {
                if !props.is_empty() {
                    let params_rule = format!("{}params", tool_name);
                    items.push(nt(&params_rule));

                    let params: Vec<Vec<_>> = props
                        .keys()
                        .map(|name| vec![t(name), t(":"), nt("value")])
                        .collect();

                    // Join parameters with ", " separator
                    let mut param_items = Vec::new();
                    for (i, param) in params.iter().enumerate() {
                        if i > 0 {
                            param_items.push(t(", "));
                        }
                        param_items.extend_from_slice(param);
                    }

                    builder = builder.rule(&params_rule, seq(&param_items));
                }
            }

            items.push(t("}"));
            builder = builder.rule(tool_name, seq(&items));
        }

        for tool_rule in &tool_rules {
            builder = builder.rule("functioncall", nt(tool_rule));
        }

        let grammar = builder
            .rule(
                "toolcall",
                seq(&[
                    t(self.begin_token()),
                    nt("ws"),
                    nt("functioncall"),
                    nt("ws"),
                    t(self.end_token()),
                    nt("ws"),
                ]),
            )
            .rule("superroot", nt("toolcall"))
            .rule("root", nt("superroot"))
            .build();

        Ok(grammar)
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
