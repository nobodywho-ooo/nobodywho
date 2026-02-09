use super::types::{Tool, ToolCall, ToolFormatError};
use super::ToolFormatHandler;
use serde_json::json;
use tracing::debug;

/// Handler for Qwen3 tool calling format.
///
/// Format:
/// - Begin token: `<tool_call>`
/// - End token: `</tool_call>`
/// - Content: JSON with `{"name": "tool_name", "arguments": {...}}`
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
        // get a json schema that describes the tool call for each tool
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

        // a json schema that describes any of the tool calls
        let tool_call_schema = json!(
            { "oneOf": tool_call_schemas }
        );

        // a GBNF grammar for the above
        let mut json_grammar = gbnf::Grammar::from_json_schema(&tool_call_schema.to_string())?;

        // optional whitespace
        let ws = gbnf::ProductionItem::NonTerminal(
            gbnf::NonTerminalSymbol { name: "ws".into() },
            gbnf::RepetitionType::One,
        );

        // wrap the newly generated grammar's root in tool calling tokens
        // e.g. <tool_call> json_grammar </tool_call>
        let tool_call_rule = gbnf::GrammarItem::Rule(gbnf::Rule {
            lhs: gbnf::NonTerminalSymbol {
                name: "toolcall".into(),
            },
            rhs: gbnf::Production {
                items: vec![
                    // tool call begin
                    gbnf::ProductionItem::Terminal(
                        gbnf::TerminalSymbol {
                            value: self.begin_token().into(),
                        },
                        gbnf::RepetitionType::One,
                    ),
                    // optional whitespace
                    ws.clone(),
                    // tool call json, just refer to the grammar we made from json schema
                    gbnf::ProductionItem::NonTerminal(
                        gbnf::NonTerminalSymbol {
                            name: "root".into(),
                        },
                        gbnf::RepetitionType::One,
                    ),
                    // optional whitespace
                    ws.clone(),
                    // </tool_call>
                    gbnf::ProductionItem::Terminal(
                        gbnf::TerminalSymbol {
                            value: self.end_token().into(),
                        },
                        gbnf::RepetitionType::One,
                    ),
                    // optional whitespace
                    ws.clone(),
                ],
            },
        });

        // one or more tool calls
        let new_root_rule = gbnf::GrammarItem::Rule(gbnf::Rule {
            lhs: gbnf::NonTerminalSymbol {
                name: "superroot".into(),
            },
            rhs: gbnf::Production {
                items: vec![gbnf::ProductionItem::NonTerminal(
                    gbnf::NonTerminalSymbol {
                        name: "toolcall".into(),
                    },
                    gbnf::RepetitionType::OneOrMore,
                )],
            },
        });

        json_grammar.items.push(tool_call_rule);
        json_grammar.items.push(new_root_rule);

        Ok(json_grammar)
    }

    fn extract_tool_calls(&self, input: &str) -> Option<Vec<ToolCall>> {
        let pattern = format!(r"{}([\s\S]*?){}", regex::escape(self.begin_token()), regex::escape(self.end_token()));
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
        assert_eq!(tool_calls[0].arguments, json!({"location": "San Francisco"}));
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
