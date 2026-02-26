//! Tool calling support for LLMs with different formats.
//!
//! This module provides abstractions for handling tool calling across different LLM formats.
//! Currently supported formats:
//! - Qwen3: `<tool_call>{"name": "...", "arguments": {...}}</tool_call>`
//! - FunctionGemma: `<start_function_call>call:name{param:<escape>val<escape>}<end_function_call>`
//! - Phi4Mini: `<|tool_calls|>[{"name": "...", "arguments": {...}}]<|/tool_calls|>`

mod functiongemma;
pub mod grammar_builder;
mod phi4mini;
mod qwen3;

use llama_cpp_2::model::LlamaModel;
use serde::{ser::Serializer, Deserialize, Serialize};
use std::sync::Arc;
use tracing::debug;

pub use functiongemma::FunctionGemmaHandler;
pub use phi4mini::Phi4MiniHandler;
pub use qwen3::Qwen3Handler;

// ============================================================================
// Core Types
// ============================================================================

/// A tool that can be called by the LLM.
#[derive(Clone)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub json_schema: serde_json::Value,
    pub function: Arc<dyn Fn(serde_json::Value) -> String + Send + Sync>,
}

impl std::fmt::Debug for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tool")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("json_schema", &self.json_schema)
            .field("function", &"<function>")
            .finish()
    }
}

impl Tool {
    pub fn new<S: Into<String>>(
        name: S,
        description: S,
        json_schema: serde_json::Value,
        function: Arc<dyn Fn(serde_json::Value) -> String + Send + Sync>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            json_schema,
            function,
        }
    }
}

// Serialize tools according to https://huggingface.co/blog/unified-tool-use
impl Serialize for Tool {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": &self.name,
                "description": &self.description,
                "parameters": &self.json_schema,
            }
        })
        .serialize(serializer)
    }
}

/// A tool call extracted from LLM output.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

// Serialize tools according to https://huggingface.co/blog/unified-tool-use
impl Serialize for ToolCall {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serde_json::json!({
            "type" : "function",
            "function": {
                "name": &self.name,
                "arguments": &self.arguments,
            }
        })
        .serialize(serializer)
    }
}

/// Errors that can occur during tool calling operations.
#[derive(Debug, thiserror::Error)]
pub enum ToolFormatError {
    #[error("Unsupported tool calling format: {0}")]
    UnsupportedFormat(String),

    #[error("Failed to detect tool calling format")]
    DetectionFailed,

    #[error("Failed to generate grammar: {0}")]
    GrammarGenerationFailed(String),

    #[error("JSON schema error: {0}")]
    JsonSchemaError(#[from] gbnf::json::JsonSchemaError),

    #[error("Lama.cpp failed fetching chat template from the model file. This is likely because you're using an older GGUF file, which might not include a chat template. For example, this is the case for most LLaMA2-based GGUF files. Try using a more recent GGUF model file. {0}")]
    ChatTemplateError(#[from] llama_cpp_2::ChatTemplateError),
}

// ============================================================================
// Trait & Format Enum
// ============================================================================

/// Trait for handling different tool calling formats.
pub trait ToolFormatHandler {
    /// Returns the token that begins a tool call (e.g., "<tool_call>")
    fn begin_token(&self) -> &str;

    /// Returns the token that ends a tool call (e.g., "</tool_call>")
    fn end_token(&self) -> &str;

    /// Generates a GBNF grammar for constrained sampling of tool calls.
    fn generate_grammar(&self, tools: &[Tool]) -> Result<gbnf::GbnfGrammar, ToolFormatError>;

    /// Extracts tool calls from the given text.
    fn extract_tool_calls(&self, input: &str) -> Option<Vec<ToolCall>>;

    /// Returns true if the chat template renders tool definitions (default).
    /// Returns false for formats that inject tools into the system prompt manually.
    fn uses_template_for_tools(&self) -> bool {
        true
    }

    /// For formats that inject tools directly into the system message content (e.g., Phi-4-mini),
    /// returns the string to append to the system message content (e.g., `<|tool|>...<|/tool|>`).
    /// Returns None if the chat template handles tool rendering via its own context variable.
    fn system_message_tool_injection(&self, _tools: &[Tool]) -> Option<String> {
        None
    }
}

/// Enum representing different tool calling formats.
#[derive(Debug, Clone)]
pub enum ToolFormat {
    Qwen3(Qwen3Handler),
    FunctionGemma(FunctionGemmaHandler),
    Phi4Mini(Phi4MiniHandler),
}

impl ToolFormat {
    pub fn handler(&self) -> &dyn ToolFormatHandler {
        match self {
            ToolFormat::Qwen3(h) => h,
            ToolFormat::FunctionGemma(h) => h,
            ToolFormat::Phi4Mini(h) => h,
        }
    }

    pub fn begin_token(&self) -> &str {
        self.handler().begin_token()
    }

    pub fn end_token(&self) -> &str {
        self.handler().end_token()
    }

    pub fn generate_grammar(&self, tools: &[Tool]) -> Result<gbnf::GbnfGrammar, ToolFormatError> {
        self.handler().generate_grammar(tools)
    }

    pub fn extract_tool_calls(&self, input: &str) -> Option<Vec<ToolCall>> {
        self.handler().extract_tool_calls(input)
    }

    pub fn uses_template_for_tools(&self) -> bool {
        self.handler().uses_template_for_tools()
    }

    pub fn system_message_tool_injection(&self, tools: &[Tool]) -> Option<String> {
        self.handler().system_message_tool_injection(tools)
    }
}

pub fn detect_tool_format(model: &LlamaModel) -> Result<ToolFormat, ToolFormatError> {
    // Fetch both the tool_use template (if any) and the default template.
    // We need them separately because Phi-4-mini's markers live in the *default* template
    // while a tool_use template (if present) may not contain them.
    let tool_use_str = model
        .chat_template(Some("tool_use"))
        .and_then(|t| Ok(t.to_string()?))
        .ok();

    let default_str = model
        .chat_template(None)
        .and_then(|t| Ok(t.to_string()?))
        .ok();

    // Require at least one template to be present.
    if tool_use_str.is_none() && default_str.is_none() {
        return Err(ToolFormatError::ChatTemplateError(
            model.chat_template(None).unwrap_err(),
        ));
    }

    // Primary template for marker checks: prefer tool_use, fall back to default.
    let primary = tool_use_str.as_deref().or(default_str.as_deref()).unwrap();
    debug!(template = %primary, "Checking primary template for format markers");

    // Check for FunctionGemma markers
    if primary.contains("<start_function_call>") || primary.contains("<end_function_call>") {
        debug!("Detected FunctionGemma format from template markers");
        return Ok(ToolFormat::FunctionGemma(FunctionGemmaHandler));
    }

    // Check for Qwen3 markers
    if primary.contains("<tool_call>") || primary.contains("</tool_call>") {
        debug!("Detected Qwen3 format from template markers");
        return Ok(ToolFormat::Qwen3(Qwen3Handler));
    }

    // Check for Phi-4-mini markers.
    // The *default* template contains '<|tool|>' / '<|/tool|>' literals (for the system message
    // tool injection path).  A separate tool_use template (if present in the GGUF) may not
    // contain them, so we always check the default template here.
    let phi4_check = tool_use_str
        .as_deref()
        .into_iter()
        .chain(default_str.as_deref())
        .any(|t| {
            t.contains("<|tool|>")
                || t.contains("<|/tool|>")
                || t.contains("<|tool_call|>")
                || t.contains("<|/tool_call|>")
        });
    if phi4_check {
        debug!("Detected Phi-4-mini format from template markers");
        return Ok(ToolFormat::Phi4Mini(Phi4MiniHandler));
    }

    // Try to detect from model name/metadata
    if let Ok(name) = model.meta_val_str("general.name") {
        debug!(model_name = %name, "Checking model name for format hints");

        let name_lower = name.to_lowercase();
        if name_lower.contains("functiongemma") || name_lower.contains("function-gemma") {
            debug!("Detected FunctionGemma format from model name");
            return Ok(ToolFormat::FunctionGemma(FunctionGemmaHandler));
        }

        if name_lower.contains("qwen") {
            debug!("Detected Qwen3 format from model name");
            return Ok(ToolFormat::Qwen3(Qwen3Handler));
        }

        if name_lower.contains("phi-4") || name_lower.contains("phi4") {
            debug!("Detected Phi-4-mini format from model name");
            return Ok(ToolFormat::Phi4Mini(Phi4MiniHandler));
        }
    }

    Err(ToolFormatError::UnsupportedFormat(
        "Cannot detect tool format from template or model family".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qwen3_format() {
        let format = ToolFormat::Qwen3(Qwen3Handler);
        assert_eq!(format.begin_token(), "<tool_call>");
        assert_eq!(format.end_token(), "</tool_call>");
    }

    #[test]
    fn test_functiongemma_format() {
        let format = ToolFormat::FunctionGemma(FunctionGemmaHandler);
        assert_eq!(format.begin_token(), "<start_function_call>");
        assert_eq!(format.end_token(), "<end_function_call>");
    }

    #[test]
    fn test_phi4mini_format() {
        let format = ToolFormat::Phi4Mini(Phi4MiniHandler);
        assert_eq!(format.begin_token(), "<|tool_call|>");
        assert_eq!(format.end_token(), "<|/tool_call|>");
        assert!(!format.uses_template_for_tools());
    }

    #[test]
    fn test_phi4mini_system_message_tools_json() {
        use serde_json::json;
        use std::sync::Arc;

        let tool = Tool {
            name: "get_weather".to_string(),
            description: "Gets weather".to_string(),
            json_schema: json!({"type": "object", "properties": {"city": {"type": "string"}}}),
            function: Arc::new(|_| "sunny".to_string()),
        };

        let format = ToolFormat::Phi4Mini(Phi4MiniHandler);
        let result = format.system_message_tool_injection(&[tool]);
        assert!(result.is_some());
        let injection = result.unwrap();
        // Should be wrapped in <|tool|>...<|/tool|>
        assert!(injection.starts_with("<|tool|>"));
        assert!(injection.ends_with("<|/tool|>"));
        let json_str = injection
            .trim_start_matches("<|tool|>")
            .trim_end_matches("<|/tool|>");
        let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed[0]["name"], "get_weather");
    }

    #[test]
    fn test_tool_serialization() {
        use serde_json::json;
        use std::sync::Arc;

        let tool = Tool {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            json_schema: json!({"type": "object"}),
            function: Arc::new(|_| "result".to_string()),
        };

        let serialized = match serde_json::to_value(&tool) {
            Ok(s) => s,
            Err(e) => panic!("Serialization of tool failed: {}", e),
        };
        assert_eq!(
            serialized,
            json!({
                "type": "function",
                "function": {
                    "name": "test_tool",
                    "description": "A test tool",
                    "parameters": {"type": "object"}
                }
            })
        );
    }

    #[test]
    fn test_tool_call_serialization() {
        use serde_json::json;

        let tool_call = ToolCall {
            name: "test_tool".to_string(),
            arguments: json!({"arg": "value"}),
        };

        let serialized = serde_json::to_value(&tool_call).unwrap();
        assert_eq!(
            serialized,
            json!({
                "type" : "function",
                "function": {
                    "name": "test_tool",
                    "arguments": {"arg": "value"}
                }
            })
        );
    }
}
