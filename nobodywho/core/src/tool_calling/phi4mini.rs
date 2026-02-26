use super::grammar_builder::{nt, seq, t, GrammarBuilder};
use super::{Tool, ToolCall, ToolFormatError, ToolFormatHandler};
use gbnf::json::json_schema_to_grammar;
use gbnf::GbnfGrammar;
use serde_json::json;
use tracing::debug;

#[derive(Debug, Clone, Copy)]
pub struct Phi4MiniHandler;

/// Converts a JSON Schema type string to the Python-style type name used in Phi-4-mini docs.
fn json_schema_type_to_phi4(type_str: &str) -> &str {
    match type_str {
        "string" => "str",
        "integer" => "int",
        "number" => "float",
        "boolean" => "bool",
        other => other,
    }
}

/// Converts a JSON Schema parameters object into Phi-4-mini's flat parameter format.
///
/// JSON Schema input:
/// ```json
/// {"type":"object","properties":{"city":{"type":"string","description":"City name"}},"required":["city"]}
/// ```
/// Phi-4-mini output:
/// ```json
/// {"city":{"type":"str","description":"City name"}}
/// ```
fn json_schema_params_to_phi4(schema: &serde_json::Value) -> serde_json::Value {
    let properties = match schema.get("properties").and_then(|p| p.as_object()) {
        Some(props) => props,
        None => return schema.clone(),
    };

    let required: Vec<&str> = schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let mut flat = serde_json::Map::new();
    for (param_name, param_schema) in properties {
        let mut info = serde_json::Map::new();

        if let Some(type_str) = param_schema.get("type").and_then(|t| t.as_str()) {
            info.insert(
                "type".to_string(),
                serde_json::Value::String(json_schema_type_to_phi4(type_str).to_string()),
            );
        }

        if let Some(desc) = param_schema.get("description") {
            info.insert("description".to_string(), desc.clone());
        }

        if let Some(default) = param_schema.get("default") {
            info.insert("default".to_string(), default.clone());
        }

        if !required.contains(&param_name.as_str()) {
            info.insert("required".to_string(), serde_json::Value::Bool(false));
        }

        flat.insert(param_name.clone(), serde_json::Value::Object(info));
    }

    serde_json::Value::Object(flat)
}

fn phi4_tool_schema(tool: &Tool) -> serde_json::Value {
    json!({
        "name": tool.name,
        "description": tool.description,
        "parameters": json_schema_params_to_phi4(&tool.json_schema)
    })
}

impl ToolFormatHandler for Phi4MiniHandler {
    fn begin_token(&self) -> &str {
        "<|tool_call|>"
    }

    fn end_token(&self) -> &str {
        "<|/tool_call|>"
    }

    fn uses_template_for_tools(&self) -> bool {
        false
    }

    fn system_message_tool_injection(&self, tools: &[Tool]) -> Option<String> {
        let phi4_tools: Vec<serde_json::Value> = tools.iter().map(phi4_tool_schema).collect();
        let json = serde_json::to_string(&phi4_tools).ok()?;
        Some(format!("<|tool|>{}<|/tool|>", json))
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

        let json_grammar = json_schema_to_grammar(tool_call_schema)?;

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
        assert!(!Phi4MiniHandler.uses_template_for_tools());
    }

    #[test]
    fn test_system_message_tool_injection() {
        let tools = vec![make_tool("get_weather")];
        let result = Phi4MiniHandler.system_message_tool_injection(&tools);
        assert!(result.is_some());
        let injection = result.unwrap();
        // Should be wrapped in <|tool|>...<|/tool|>
        assert!(injection.starts_with("<|tool|>"));
        assert!(injection.ends_with("<|/tool|>"));
        assert!(injection.contains("get_weather"));
        assert!(injection.contains("A test tool"));

        // Parameters should use the flat Phi-4-mini format, not JSON Schema
        let json_str = injection
            .trim_start_matches("<|tool|>")
            .trim_end_matches("<|/tool|>");
        let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let params = &parsed[0]["parameters"];
        // Flat format: {"arg": {"type": "str"}} — NOT {"type": "object", "properties": {...}}
        assert!(
            params.get("type").is_none(),
            "parameters should not have a top-level 'type' key"
        );
        assert!(
            params.get("arg").is_some(),
            "parameters should have 'arg' as a direct key"
        );
        assert_eq!(params["arg"]["type"], "str");
    }

    #[test]
    fn test_json_schema_params_to_phi4() {
        let schema = json!({
            "type": "object",
            "properties": {
                "city": {"type": "string", "description": "City name", "default": "London"},
                "units": {"type": "string", "description": "Temperature units"}
            },
            "required": ["city"]
        });
        let result = json_schema_params_to_phi4(&schema);
        assert_eq!(result["city"]["type"], "str");
        assert_eq!(result["city"]["description"], "City name");
        assert_eq!(result["city"]["default"], "London");
        // "city" is required so no "required": false
        assert!(result["city"].get("required").is_none());
        // "units" is not required
        assert_eq!(result["units"]["required"], false);
        assert_eq!(result["units"]["type"], "str");
    }

    #[test]
    fn test_json_schema_type_mapping() {
        assert_eq!(json_schema_type_to_phi4("string"), "str");
        assert_eq!(json_schema_type_to_phi4("integer"), "int");
        assert_eq!(json_schema_type_to_phi4("number"), "float");
        assert_eq!(json_schema_type_to_phi4("boolean"), "bool");
        assert_eq!(json_schema_type_to_phi4("array"), "array");
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
