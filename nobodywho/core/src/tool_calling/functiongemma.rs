use super::types::{Tool, ToolCall, ToolFormatError};
use super::ToolFormatHandler;
use serde_json::json;
use tracing::debug;

/// Handler for FunctionGemma tool calling format.
///
/// Format:
/// - Begin token: `<start_function_call>`
/// - End token: `<end_function_call>`
/// - Content: `call:function_name{param1:<escape>value1<escape>, param2:<escape>value2<escape>}`
///
/// Note: FunctionGemma may require "developer" role for system messages in the chat template.
#[derive(Debug, Clone, Copy)]
pub struct FunctionGemmaHandler;

/// Helper module for building GBNF grammars with less boilerplate
mod grammar_builder {
    use gbnf::{
        CharacterSet, CharacterSetItem, Grammar, GrammarItem, NonTerminalSymbol, Production,
        ProductionItem, RepetitionType, Rule, TerminalSymbol,
    };

    pub struct GrammarBuilder {
        grammar: Grammar,
    }

    impl GrammarBuilder {
        pub fn new() -> Self {
            Self {
                grammar: Grammar::default(),
            }
        }

        /// Add a rule to the grammar
        pub fn rule(mut self, name: &str, items: Vec<ProductionItem>) -> Self {
            self.grammar.items.push(GrammarItem::Rule(Rule {
                lhs: NonTerminalSymbol { name: name.into() },
                rhs: Production { items },
            }));
            self
        }

        /// Add a recurring rule (stored separately in grammar.recurring_items)
        pub fn recurring_rule(mut self, name: &str, items: Vec<ProductionItem>) -> Self {
            self.grammar.recurring_items.insert(
                NonTerminalSymbol { name: name.into() },
                Production { items },
            );
            self
        }

        pub fn build(self) -> Grammar {
            self.grammar
        }
    }

    /// Create a terminal production item (exact text match)
    pub fn t(s: &str) -> ProductionItem {
        ProductionItem::Terminal(TerminalSymbol { value: s.into() }, RepetitionType::One)
    }

    /// Create a non-terminal reference with optional repetition
    pub fn nt(name: &str) -> ProductionItem {
        ProductionItem::NonTerminal(NonTerminalSymbol { name: name.into() }, RepetitionType::One)
    }

    /// Create a terminal with zero-or-more repetition
    pub fn t_star(s: &str) -> ProductionItem {
        ProductionItem::Terminal(
            TerminalSymbol { value: s.into() },
            RepetitionType::ZeroOrMore,
        )
    }

    /// Create a character set that matches anything except the given characters
    pub fn not_chars(chars: &[char]) -> ProductionItem {
        ProductionItem::CharacterSet(
            CharacterSet {
                is_complement: true,
                items: chars
                    .iter()
                    .map(|&c| CharacterSetItem::Character(c))
                    .collect(),
            },
            RepetitionType::OneOrMore,
        )
    }
}

impl ToolFormatHandler for FunctionGemmaHandler {
    fn begin_token(&self) -> &str {
        "<start_function_call>"
    }

    fn end_token(&self) -> &str {
        "<end_function_call>"
    }

    fn generate_grammar(&self, tools: &[Tool]) -> Result<gbnf::Grammar, ToolFormatError> {
        use grammar_builder::{not_chars, nt, t, t_star, GrammarBuilder};

        // FunctionGemma format: call:tool_name{param1:<escape>value<escape>, param2:<escape>value<escape>}
        let mut builder = GrammarBuilder::new()
            .rule("ws", vec![t_star(" ")])
            // Value content: exclude special chars that have meaning in FunctionGemma syntax
            .recurring_rule(
                "valuecontent",
                vec![not_chars(&['<', '>', '{', '}', ',', ':'])],
            )
            .rule(
                "value",
                vec![t("<escape>"), nt("valuecontent"), t("<escape>")],
            );

        // Generate a rule for each tool using the actual function name
        let tool_rules: Vec<_> = tools
            .iter()
            .map(|tool| {
                // Sanitize the tool name for GBNF (only alphanumeric allowed, no underscores)
                // Remove all non-alphanumeric characters
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

                    // Build parameter list: param1:<escape>val<escape>, param2:<escape>val<escape>
                    let param_items: Vec<_> = props
                        .keys()
                        .enumerate()
                        .flat_map(|(i, name)| {
                            let mut items = vec![t(name), t(":"), nt("value")];
                            if i > 0 {
                                items.insert(0, t(", "));
                            }
                            items
                        })
                        .collect();

                    builder = builder.rule(&params_rule, param_items);
                }
            }

            items.push(t("}"));
            builder = builder.rule(tool_name, items);
        }

        for tool_rule in &tool_rules {
            builder = builder.rule("functioncall", vec![nt(tool_rule)]);
        }

        let grammar = builder
            .rule(
                "toolcall",
                vec![
                    t(self.begin_token()),
                    nt("ws"),
                    nt("functioncall"),
                    nt("ws"),
                    t(self.end_token()),
                    nt("ws"),
                ],
            )
            .rule("superroot", vec![nt("toolcall")])
            .rule("root", vec![nt("superroot")])
            .build();

        Ok(grammar)
    }

    fn extract_tool_calls(&self, input: &str) -> Option<Vec<ToolCall>> {
        // Parse FunctionGemma format: call:function_name{param1:<escape>value1<escape>, param2:<escape>value2<escape>}
        let pattern = format!(
            r"{}([\s\S]*?){}",
            regex::escape(self.begin_token()),
            regex::escape(self.end_token())
        );
        let re = regex::Regex::new(&pattern).expect("Invalid regex");

        let tool_calls: Vec<ToolCall> = re
            .captures_iter(input)
            .filter_map(|cap| {
                let content = cap[1].trim();

                // Parse the FunctionGemma format
                if !content.starts_with("call:") {
                    debug!(
                        content = content,
                        "FunctionGemma content doesn't start with 'call:'"
                    );
                    return None;
                }

                let content = &content[5..]; // Skip "call:"

                // Find the function name (everything before '{')
                let (name, params_str) = if let Some(brace_pos) = content.find('{') {
                    let name = content[..brace_pos].trim();
                    let params = &content[brace_pos + 1..];
                    // Remove trailing '}'
                    let params = params.strip_suffix('}').unwrap_or(params);
                    (name, params)
                } else {
                    // No parameters
                    (content.trim(), "")
                };

                // Parse parameters
                let mut arguments = json!({});

                if !params_str.is_empty() {
                    // Split by commas outside of <escape> blocks
                    let params: Vec<&str> = split_params(params_str);

                    for param in params {
                        if let Some(colon_pos) = param.find(':') {
                            let key = param[..colon_pos].trim();
                            let value_str = param[colon_pos + 1..].trim();

                            // Remove <escape> tags
                            let value_str = value_str
                                .trim_start_matches("<escape>")
                                .trim_end_matches("<escape>");

                            // Try to parse as JSON, fallback to string
                            let value = serde_json::from_str(value_str)
                                .unwrap_or_else(|_| json!(value_str));

                            arguments[key] = value;
                        }
                    }
                }

                let tool_call = ToolCall {
                    name: name.to_string(),
                    arguments,
                };

                debug!(tool_name = %tool_call.name, "Successfully parsed FunctionGemma tool call");
                Some(tool_call)
            })
            .collect();

        if !tool_calls.is_empty() {
            Some(tool_calls)
        } else {
            debug!("No FunctionGemma tool calls detected in message");
            None
        }
    }

    fn serialize_tool(&self, tool: &Tool) -> serde_json::Value {
        // FunctionGemma uses same tool definition format as Qwen3/OpenAI
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
        // FunctionGemma wraps tool_calls in "function" object
        json!({
            "function": {
                "name": &tool_call.name,
                "arguments": &tool_call.arguments,
            }
        })
    }
}

/// Split parameters by comma, but not commas inside <escape> blocks
/// Note: FunctionGemma uses <escape>value<escape> (not </escape>)
fn split_params(params_str: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut current_start = 0;
    let mut in_escape = false;
    let escape_tag = "<escape>";

    let mut i = 0;
    while i < params_str.len() {
        if params_str[i..].starts_with(escape_tag) {
            in_escape = !in_escape; // Toggle escape state
            i += escape_tag.len();
        } else if params_str.as_bytes()[i] == b',' && !in_escape {
            result.push(params_str[current_start..i].trim());
            current_start = i + 1;
            i += 1;
        } else {
            i += 1;
        }
    }

    // Add the last parameter
    if current_start < params_str.len() {
        result.push(params_str[current_start..].trim());
    }

    result
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
