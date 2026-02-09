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

impl ToolFormatHandler for FunctionGemmaHandler {
    fn begin_token(&self) -> &str {
        "<start_function_call>"
    }

    fn end_token(&self) -> &str {
        "<end_function_call>"
    }

    fn generate_grammar(&self, tools: &[Tool]) -> Result<gbnf::Grammar, ToolFormatError> {
        // FunctionGemma format: call:tool_name{param1:<escape>value<escape>, param2:<escape>value<escape>}

        let mut grammar = gbnf::Grammar::default();

        // Helper to create terminal
        let term = |s: &str| -> gbnf::ProductionItem {
            gbnf::ProductionItem::Terminal(
                gbnf::TerminalSymbol { value: s.to_string() },
                gbnf::RepetitionType::One,
            )
        };

        // Helper to create non-terminal
        let nonterm = |s: &str, rep: gbnf::RepetitionType| -> gbnf::ProductionItem {
            gbnf::ProductionItem::NonTerminal(
                gbnf::NonTerminalSymbol { name: s.to_string() },
                rep,
            )
        };

        // Define whitespace rule (optional space)
        grammar.items.push(gbnf::GrammarItem::Rule(gbnf::Rule {
            lhs: gbnf::NonTerminalSymbol { name: "ws".into() },
            rhs: gbnf::Production {
                items: vec![
                    gbnf::ProductionItem::Terminal(
                        gbnf::TerminalSymbol { value: " ".into() },
                        gbnf::RepetitionType::ZeroOrMore,
                    ),
                ],
            },
        }));

        // Value inside <escape> tags
        // Define a very permissive character set using complement (anything except escape tags)
        // This is simpler and avoids character escaping issues

        // Add the valuecontent rule to recurring_items so it's defined once
        let valuecontent_rule = gbnf::Production {
            items: vec![
                gbnf::ProductionItem::CharacterSet(
                    gbnf::CharacterSet {
                        is_complement: true,  // Use complement: match anything EXCEPT these characters
                        items: vec![
                            gbnf::CharacterSetItem::Character('<'),  // Don't match < to avoid tag conflicts
                        ],
                    },
                    gbnf::RepetitionType::OneOrMore,
                ),
            ],
        };
        grammar.recurring_items.insert(
            gbnf::NonTerminalSymbol { name: "valuecontent".into() },
            valuecontent_rule,
        );

        grammar.items.push(gbnf::GrammarItem::Rule(gbnf::Rule {
            lhs: gbnf::NonTerminalSymbol { name: "value".into() },
            rhs: gbnf::Production {
                items: vec![
                    term("<escape>"),
                    nonterm("valuecontent", gbnf::RepetitionType::One),
                    term("<escape>"),
                ],
            },
        }));

        // Generate rules for each tool
        let mut tool_rules = Vec::new();

        for (idx, tool) in tools.iter().enumerate() {
            // Use letter-based naming to avoid GBNF parsing issues with underscores and digits
            // tool0, tool1, etc. -> toola, toolb, toolc, ...
            let tool_letter = char::from_u32('a' as u32 + idx as u32).unwrap_or('z');
            let tool_rule_name = format!("tool{}", tool_letter);
            tool_rules.push(tool_rule_name.clone());

            // Get parameters from JSON schema
            let properties = tool.json_schema
                .get("properties")
                .and_then(|p| p.as_object());

            // Build the tool call: call:toolname{params}
            let mut tool_items = vec![
                term("call:"),
                term(&tool.name),
                term("{"),
            ];

            if let Some(props) = properties {
                if !props.is_empty() {
                    // Generate params rule for this tool (e.g., "toolaparams", "toolbparams")
                    let params_rule_name = format!("{}params", tool_rule_name);
                    tool_items.push(nonterm(&params_rule_name, gbnf::RepetitionType::One));

                    // Create parameter list rule
                    let param_names: Vec<_> = props.keys().collect();

                    if !param_names.is_empty() {
                        // Generate rules for each parameter
                        let mut param_items = Vec::new();

                        for (i, param_name) in param_names.iter().enumerate() {
                            // Add comma and space before all params except the first
                            if i > 0 {
                                param_items.push(term(", "));
                            }

                            // param_name:<escape>value<escape>
                            param_items.push(term(param_name));
                            param_items.push(term(":"));
                            param_items.push(nonterm("value", gbnf::RepetitionType::One));
                        }

                        grammar.items.push(gbnf::GrammarItem::Rule(gbnf::Rule {
                            lhs: gbnf::NonTerminalSymbol { name: params_rule_name },
                            rhs: gbnf::Production { items: param_items },
                        }));
                    }
                }
            }

            tool_items.push(term("}"));

            grammar.items.push(gbnf::GrammarItem::Rule(gbnf::Rule {
                lhs: gbnf::NonTerminalSymbol { name: tool_rule_name },
                rhs: gbnf::Production { items: tool_items },
            }));
        }

        // Create a choice rule for all tools (multiple productions with same LHS)
        for tool_rule in tool_rules {
            grammar.items.push(gbnf::GrammarItem::Rule(gbnf::Rule {
                lhs: gbnf::NonTerminalSymbol { name: "functioncall".into() },
                rhs: gbnf::Production {
                    items: vec![
                        nonterm(&tool_rule, gbnf::RepetitionType::One),
                    ],
                },
            }));
        }

        // Optional whitespace
        let ws = nonterm("ws", gbnf::RepetitionType::One);

        // Single tool call wrapped in tags: <start_function_call>...function call...<end_function_call>
        grammar.items.push(gbnf::GrammarItem::Rule(gbnf::Rule {
            lhs: gbnf::NonTerminalSymbol { name: "toolcall".into() },
            rhs: gbnf::Production {
                items: vec![
                    term(self.begin_token()),
                    ws.clone(),
                    nonterm("functioncall", gbnf::RepetitionType::One),
                    ws.clone(),
                    term(self.end_token()),
                    ws.clone(),
                ],
            },
        }));

        // Superroot: exactly one tool call (model can emit EOS after completing it)
        grammar.items.push(gbnf::GrammarItem::Rule(gbnf::Rule {
            lhs: gbnf::NonTerminalSymbol { name: "superroot".into() },
            rhs: gbnf::Production {
                items: vec![
                    nonterm("toolcall", gbnf::RepetitionType::One),
                ],
            },
        }));

        // Add a 'root' rule that llama.cpp expects (even though we use superroot as entry point)
        grammar.items.push(gbnf::GrammarItem::Rule(gbnf::Rule {
            lhs: gbnf::NonTerminalSymbol { name: "root".into() },
            rhs: gbnf::Production {
                items: vec![
                    nonterm("superroot", gbnf::RepetitionType::One),
                ],
            },
        }));

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
                    debug!(content = content, "FunctionGemma content doesn't start with 'call:'");
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

    fn transform_message_for_template(&self, mut message: serde_json::Value) -> serde_json::Value {
        // FunctionGemma templates expect tool_calls to be wrapped in a 'function' object:
        // {"function": {"name": "...", "arguments": {...}}}
        // instead of just {"name": "...", "arguments": {...}}

        if let Some(tool_calls) = message.get_mut("tool_calls") {
            if let Some(tool_calls_array) = tool_calls.as_array_mut() {
                for tool_call in tool_calls_array.iter_mut() {
                    if let Some(obj) = tool_call.as_object() {
                        // Only transform if not already wrapped
                        if !obj.contains_key("function") {
                            let name = obj.get("name").cloned();
                            let arguments = obj.get("arguments").cloned();

                            if let (Some(name), Some(arguments)) = (name, arguments) {
                                *tool_call = json!({
                                    "function": {
                                        "name": name,
                                        "arguments": arguments,
                                    }
                                });
                            }
                        }
                    }
                }
            }
        }

        message
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
        assert_eq!(tool_calls[0].arguments, json!({"location": "San Francisco"}));
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

    #[test]
    fn test_split_params() {
        let params = "x:<escape>10<escape>, y:<escape>20<escape>";
        let result = split_params(params);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "x:<escape>10<escape>");
        assert_eq!(result[1], "y:<escape>20<escape>");
    }

    #[test]
    fn test_split_params_with_comma_in_escape() {
        let params = "text:<escape>hello, world<escape>, num:<escape>42<escape>";
        let result = split_params(params);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "text:<escape>hello, world<escape>");
        assert_eq!(result[1], "num:<escape>42<escape>");
    }

    #[test]
    fn test_functiongemma_grammar_generation() {
        use std::sync::Arc;

        let handler = FunctionGemmaHandler;

        // Create a simple tool for testing
        let tool = Tool::new(
            "get_weather",
            "Get the weather for a location",
            json!({
                "type": "object",
                "properties": {
                    "location": {
                        "type": "string",
                        "description": "The city name"
                    },
                    "unit": {
                        "type": "string",
                        "description": "Temperature unit"
                    }
                },
                "required": ["location"]
            }),
            Arc::new(|_args| "Sunny, 72°F".to_string()),
        );

        // Test grammar generation
        let result = handler.generate_grammar(&[tool]);
        assert!(result.is_ok(), "Grammar generation should succeed");

        let grammar = result.unwrap();
        assert!(!grammar.items.is_empty(), "Grammar should have items");

        // Convert to string to ensure it's valid
        let _grammar_str = grammar.to_string();

        // The grammar should have rules for the tool - check for existence rather than exact format
        assert!(!grammar.items.is_empty(), "Grammar should have rules");

        // Verify by looking for rules that reference our tool
        let has_tool_rule = grammar.items.iter().any(|item| {
            if let gbnf::GrammarItem::Rule(rule) = item {
                rule.lhs.name.contains("tool_") ||
                rule.rhs.items.iter().any(|prod_item| {
                    if let gbnf::ProductionItem::Terminal(term, _) = prod_item {
                        term.value.contains("get_weather")
                    } else {
                        false
                    }
                })
            } else {
                false
            }
        });

        assert!(has_tool_rule, "Grammar should contain rules for get_weather tool");
    }

    #[test]
    fn test_functiongemma_grammar_multiple_tools() {
        use std::sync::Arc;

        let handler = FunctionGemmaHandler;

        let tool1 = Tool::new(
            "add",
            "Add two numbers",
            json!({
                "type": "object",
                "properties": {
                    "a": { "type": "number" },
                    "b": { "type": "number" }
                },
                "required": ["a", "b"]
            }),
            Arc::new(|_args| "42".to_string()),
        );

        let tool2 = Tool::new(
            "multiply",
            "Multiply two numbers",
            json!({
                "type": "object",
                "properties": {
                    "x": { "type": "number" },
                    "y": { "type": "number" }
                },
                "required": ["x", "y"]
            }),
            Arc::new(|_args| "100".to_string()),
        );

        let result = handler.generate_grammar(&[tool1, tool2]);
        assert!(result.is_ok(), "Grammar generation should succeed for multiple tools");

        let grammar = result.unwrap();

        // Verify both tools are in the grammar
        let has_add = grammar.items.iter().any(|item| {
            if let gbnf::GrammarItem::Rule(rule) = item {
                rule.rhs.items.iter().any(|prod_item| {
                    if let gbnf::ProductionItem::Terminal(term, _) = prod_item {
                        term.value.contains("add")
                    } else {
                        false
                    }
                })
            } else {
                false
            }
        });

        let has_multiply = grammar.items.iter().any(|item| {
            if let gbnf::GrammarItem::Rule(rule) = item {
                rule.rhs.items.iter().any(|prod_item| {
                    if let gbnf::ProductionItem::Terminal(term, _) = prod_item {
                        term.value.contains("multiply")
                    } else {
                        false
                    }
                })
            } else {
                false
            }
        });

        assert!(has_add, "Grammar should contain add tool");
        assert!(has_multiply, "Grammar should contain multiply tool");
    }

    #[test]
    fn test_functiongemma_end_to_end() {
        use std::sync::Arc;

        let handler = FunctionGemmaHandler;

        // Create a tool
        let tool = Tool::new(
            "calculate",
            "Perform a calculation",
            json!({
                "type": "object",
                "properties": {
                    "operation": { "type": "string" },
                    "x": { "type": "number" },
                    "y": { "type": "number" }
                },
                "required": ["operation", "x", "y"]
            }),
            Arc::new(|_args| "Result: 42".to_string()),
        );

        // Test 1: Generate grammar
        let grammar_result = handler.generate_grammar(&[tool]);
        assert!(grammar_result.is_ok(), "Grammar generation should succeed");

        // Test 2: Parse tool call
        let input = r#"<start_function_call>call:calculate{operation:<escape>add<escape>, x:<escape>10<escape>, y:<escape>32<escape>}<end_function_call>"#;
        let extract_result = handler.extract_tool_calls(input);
        assert!(extract_result.is_some(), "Should extract tool call");

        let tool_calls = extract_result.unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "calculate");
        assert_eq!(tool_calls[0].arguments["operation"], json!("add"));
        assert_eq!(tool_calls[0].arguments["x"], json!(10));
        assert_eq!(tool_calls[0].arguments["y"], json!(32));
    }

    #[test]
    fn test_functiongemma_grammar_has_superroot() {
        use std::sync::Arc;

        let handler = FunctionGemmaHandler;

        let tool = Tool::new(
            "test",
            "Test tool",
            json!({
                "type": "object",
                "properties": {
                    "param": { "type": "string" }
                }
            }),
            Arc::new(|_| "ok".to_string()),
        );

        let grammar = handler.generate_grammar(&[tool]).unwrap();

        // Verify the grammar has a "superroot" rule (required by chat.rs)
        let has_superroot = grammar.items.iter().any(|item| {
            if let gbnf::GrammarItem::Rule(rule) = item {
                rule.lhs.name == "superroot"
            } else {
                false
            }
        });

        assert!(has_superroot, "Grammar must have a 'superroot' rule for compatibility with chat.rs");

        // Also verify it has a toolcall rule
        let has_toolcall = grammar.items.iter().any(|item| {
            if let gbnf::GrammarItem::Rule(rule) = item {
                rule.lhs.name == "toolcall"
            } else {
                false
            }
        });

        assert!(has_toolcall, "Grammar must have a 'toolcall' rule");
    }

    #[test]
    fn test_functiongemma_grammar_string_output() {
        use std::sync::Arc;

        let handler = FunctionGemmaHandler;

        let tool = Tool::new(
            "get_weather",
            "Get weather",
            json!({
                "type": "object",
                "properties": {
                    "location": { "type": "string" }
                }
            }),
            Arc::new(|_| "ok".to_string()),
        );

        let grammar = handler.generate_grammar(&[tool]).unwrap();
        let grammar_str = grammar.to_string();

        println!("=== FunctionGemma Grammar ===");
        println!("{}", grammar_str);
        println!("=== End Grammar ===");

        // Basic sanity checks
        assert!(!grammar_str.is_empty(), "Grammar string should not be empty");
        assert!(grammar_str.contains("superroot"), "Grammar should contain superroot");
        assert!(grammar_str.contains("toolcall"), "Grammar should contain toolcall");
    }
}
