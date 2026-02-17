//! Tool calling support for LLMs with different formats.
//!
//! This module provides abstractions for handling tool calling across different LLM formats.
//! Currently supported formats:
//! - Qwen3: `<tool_call>{"name": "...", "arguments": {...}}</tool_call>`
//! - FunctionGemma: `<start_function_call>call:name{param:<escape>val<escape>}<end_function_call>`

mod functiongemma;
pub mod grammar_builder;
mod qwen3;

use llama_cpp_2::model::LlamaModel;
use serde::{ser::Serializer, Deserialize, Serialize};
use std::sync::Arc;
use tracing::debug;

pub use functiongemma::FunctionGemmaHandler;
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

impl Serialize for ToolCall {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serde_json::json!({
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
}

/// Enum representing different tool calling formats.
#[derive(Debug, Clone)]
pub enum ToolFormat {
    Qwen3(Qwen3Handler),
    FunctionGemma(FunctionGemmaHandler),
}

impl ToolFormat {
    pub fn handler(&self) -> &dyn ToolFormatHandler {
        match self {
            ToolFormat::Qwen3(h) => h,
            ToolFormat::FunctionGemma(h) => h,
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
}

pub fn detect_tool_format(model: &LlamaModel) -> Result<ToolFormat, ToolFormatError> {
    // get a chat template from the model
    // fails early if no utf-8 decodable chat template is found
    let template_str = model
        // 1. try to get the "tool_use" chat template if present
        .chat_template(Some("tool_use"))
        .and_then(|t| Ok(t.to_string()?))
        // 2. try to get the default chat template if no tool_use chat template
        .or_else(|_| model.chat_template(None).and_then(|t| Ok(t.to_string()?)))?;

    debug!(template = %template_str, "Checking template for format markers");

    // Check for FunctionGemma markers
    if template_str.contains("<start_function_call>")
        || template_str.contains("<end_function_call>")
    {
        debug!("Detected FunctionGemma format from template markers");
        return Ok(ToolFormat::FunctionGemma(FunctionGemmaHandler));
    }

    // Check for Qwen3 markers
    if template_str.contains("<tool_call>") || template_str.contains("</tool_call>") {
        debug!("Detected Qwen3 format from template markers");
        return Ok(ToolFormat::Qwen3(Qwen3Handler));
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
                "function": {
                    "name": "test_tool",
                    "arguments": {"arg": "value"}
                }
            })
        );
    }
}
