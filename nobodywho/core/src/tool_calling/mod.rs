//! Tool calling support for LLMs with different formats.
//!
//! This module provides abstractions for handling tool calling across different LLM formats.
//! Currently supported formats:
//! - Qwen3: `<tool_call>{"name": "...", "arguments": {...}}</tool_call>`
//! - FunctionGemma: `<start_function_call>call:name{param:<escape>val<escape>}<end_function_call>`

mod functiongemma;
mod qwen3;
pub mod types;

use llama_cpp_2::model::LlamaModel;
use tracing::debug;

pub use functiongemma::FunctionGemmaHandler;
pub use qwen3::Qwen3Handler;
pub use types::{Tool, ToolCall, ToolFormatError};

/// Trait for handling different tool calling formats.
pub trait ToolFormatHandler {
    /// Returns the token that begins a tool call (e.g., "<tool_call>")
    fn begin_token(&self) -> &str;

    /// Returns the token that ends a tool call (e.g., "</tool_call>")
    fn end_token(&self) -> &str;

    /// Generates a GBNF grammar for constrained sampling of tool calls.
    fn generate_grammar(&self, tools: &[Tool]) -> Result<gbnf::Grammar, ToolFormatError>;

    /// Extracts tool calls from the given text.
    fn extract_tool_calls(&self, input: &str) -> Option<Vec<ToolCall>>;

    /// Transform a message for template compatibility.
    /// Some formats (like FunctionGemma) require tool_calls to be wrapped in a 'function' object.
    /// Default implementation returns the message unchanged (suitable for Qwen3).
    fn transform_message_for_template(&self, message: serde_json::Value) -> serde_json::Value {
        message
    }
}

/// Enum representing different tool calling formats.
#[derive(Debug, Clone)]
pub enum ToolFormat {
    /// Qwen3 format: `<tool_call>{"name": "...", "arguments": {...}}</tool_call>`
    Qwen3(Qwen3Handler),

    /// FunctionGemma format: `<start_function_call>call:name{param:<escape>val<escape>}<end_function_call>`
    FunctionGemma(FunctionGemmaHandler),
}

impl ToolFormat {
    /// Get the handler for this format as a trait object.
    fn handler(&self) -> &dyn ToolFormatHandler {
        match self {
            ToolFormat::Qwen3(h) => h,
            ToolFormat::FunctionGemma(h) => h,
        }
    }

    /// Returns the token that begins a tool call.
    pub fn begin_token(&self) -> &str {
        self.handler().begin_token()
    }

    /// Returns the token that ends a tool call.
    pub fn end_token(&self) -> &str {
        self.handler().end_token()
    }

    /// Generates a GBNF grammar for constrained sampling.
    pub fn generate_grammar(&self, tools: &[Tool]) -> Result<gbnf::Grammar, ToolFormatError> {
        self.handler().generate_grammar(tools)
    }

    /// Extracts tool calls from the given text.
    pub fn extract_tool_calls(&self, input: &str) -> Option<Vec<ToolCall>> {
        self.handler().extract_tool_calls(input)
    }

    /// Transform a message for template compatibility.
    pub fn transform_message_for_template(&self, message: serde_json::Value) -> serde_json::Value {
        self.handler().transform_message_for_template(message)
    }
}

/// Detects the tool calling format for the given model.
///
/// Detection strategy:
/// 1. Check chat template for format-specific markers (most reliable)
/// 2. Fall back to model metadata/name patterns
/// 3. Default to Qwen3 for backward compatibility
pub fn detect_tool_format(model: &LlamaModel) -> Result<ToolFormat, ToolFormatError> {
    // Try to get tool_use template
    if let Ok(template) = model.chat_template(Some("tool_use")) {
        if let Ok(template_str) = template.to_string() {
            debug!(template = %template_str, "Checking tool_use template for format markers");

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
        }
    }

    // Fallback: check default chat template
    if let Ok(template) = model.chat_template(None) {
        if let Ok(template_str) = template.to_string() {
            debug!(template = %template_str, "Checking default template for format markers");

            if template_str.contains("<start_function_call>")
                || template_str.contains("<end_function_call>")
            {
                debug!("Detected FunctionGemma format from default template");
                return Ok(ToolFormat::FunctionGemma(FunctionGemmaHandler));
            }

            if template_str.contains("<tool_call>") || template_str.contains("</tool_call>") {
                debug!("Detected Qwen3 format from default template");
                return Ok(ToolFormat::Qwen3(Qwen3Handler));
            }
        }
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

    // Default to Qwen3 for backward compatibility
    debug!("No specific format detected, defaulting to Qwen3 format");
    Ok(ToolFormat::Qwen3(Qwen3Handler))
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
}
