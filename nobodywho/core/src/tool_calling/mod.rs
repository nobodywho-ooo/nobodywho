//! Tool calling support for LLMs with different formats.
//!
//! This module provides abstractions for handling tool calling across different LLM formats.
//! Currently supported formats:
//! - Qwen3: `<tool_call>{"name": "...", "arguments": {...}}</tool_call>`
//! - FunctionGemma: `<start_function_call>call:name{param:<escape>val<escape>}<end_function_call>`
//! - Phi4Mini: `<|tool_call|>{"name": "...", "arguments": {...}}<|/tool_call|>`

mod functiongemma;
mod ministral3;
mod phi4mini;
mod qwen3;

use bashkit::{ExecutionLimits, InMemoryFs};
use llama_cpp_2::model::LlamaModel;
use monty::{LimitedTracker, MontyRun, PrintWriter, ResourceLimits};
use serde::{ser::Serializer, Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use tracing::debug;

pub use functiongemma::FunctionGemmaHandler;
pub use ministral3::Ministral3Handler;
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

    pub fn python(
        max_duration: Option<Duration>,
        max_memory: Option<usize>,
        max_recursion_depth: Option<usize>,
    ) -> Self {
        Tool::new(
            "run_python",
            "Run a Python snippet and return its printed output. All values must be hardcoded in the code.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "
                        Self-contained Python code with all values hardcoded. Use print() to produce output.
                        Limitations of the Python interpreter:
                        - No class definitions (use dicts or plain variables instead)
                        - No match statements (use if/elif chains instead)
                        - No third-party libraries (no numpy, requests, etc.)
                        - Standard library is limited to: sys, os, typing, asyncio, re
                        - No direct filesystem, network, or environment variable access
                        "
                    }
                },
                "required": ["code"]
            }),
            Arc::new({
                move |args: serde_json::Value| -> String {
                    let Some(code) = args.get("code").and_then(|c| c.as_str()) else {
                        return "ERROR: Code parameter could not be extracted".to_string();
                    };

                    let runner = match MontyRun::new(code.to_string(), "script.py", vec![], vec![]) {
                        Ok(runner) => runner,
                        Err(e) => return format!("ERROR: Failed to create Python runner: {e}"),
                    };

                    let mut output = PrintWriter::Collect(String::new());
                    let limits = ResourceLimits {
                        max_duration,
                        max_memory,
                        gc_interval: None, // we dont let the user configure this
                        max_allocations: None, // we dont let the user configure this
                        max_recursion_depth,
                    };

                    match runner.run(vec![], LimitedTracker::new(limits), &mut output) {
                        Ok(_) => output.collected_output().unwrap_or_default().to_string(),
                        Err(e) => format!("ERROR: Failed to run Python code: {e}"),
                    }
                }
            }),
        )
    }

    pub fn bash(max_commands: Option<usize>) -> Self {
        Tool::new(
            "run_bash",
            "Run a bash snippet and return its stdout (and stderr if non-empty). All values must be hardcoded in the commands.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "commands": {
                        "type": "string",
                        "description": "
                        Self-contained bash commands with all values hardcoded.
                        Limitations of the bash interpreter:
                        - In-memory filesystem only (no persistent state between calls)
                        - No network access
                        - No access to host environment variables or host filesystem
                        "
                    }
                },
                "required": ["commands"]
            }),
            Arc::new({
                move |args: serde_json::Value| -> String {
                    let Some(commands) = args.get("commands").and_then(|c| c.as_str()) else {
                        return "ERROR: commands parameter could not be extracted".to_string();
                    };

                    // bashkit requires a Tokio reactor (for timers, I/O, etc.),
                    // so we need a Tokio runtime here rather than futures::executor.
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("failed to create tokio runtime for bash tool");
                    rt.block_on(async {
                        let fs = std::sync::Arc::new(InMemoryFs::new());
                        let limits = if let Some(max_cmds) = max_commands {
                            ExecutionLimits::new().max_commands(max_cmds)
                        } else {
                            ExecutionLimits::new()
                        };
                        let mut bash = bashkit::Bash::builder().fs(fs).limits(limits).build();

                        match bash.exec(commands).await {
                            Ok(result) => {
                                let mut output = result.stdout;
                                if !result.stderr.is_empty() {
                                    if !output.is_empty() {
                                        output.push('\n');
                                    }
                                    output.push_str("STDERR: ");
                                    output.push_str(&result.stderr);
                                }
                                output
                            }
                            Err(e) => format!("ERROR: {e}"),
                        }
                    })
                }
            }),
        )
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
}

/// Enum representing different tool calling formats.
#[derive(Debug, Clone)]
pub enum ToolFormat {
    Qwen3(Qwen3Handler),
    FunctionGemma(FunctionGemmaHandler),
    Ministral3(Ministral3Handler),
    Phi4Mini(Phi4MiniHandler),
}

impl ToolFormat {
    pub fn handler(&self) -> &dyn ToolFormatHandler {
        match self {
            ToolFormat::Qwen3(h) => h,
            ToolFormat::FunctionGemma(h) => h,
            ToolFormat::Ministral3(h) => h,
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
}

pub fn detect_tool_format(model: &LlamaModel) -> Result<ToolFormat, ToolFormatError> {
    let template = model
        .chat_template(Some("tool_use"))
        .or_else(|_| model.chat_template(None))
        .and_then(|t| Ok(t.to_string()?))?;

    debug!(template = %template, "Checking template for format markers");

    if template.contains("<start_function_call>") || template.contains("<end_function_call>") {
        debug!("Detected FunctionGemma format from template markers");
        return Ok(ToolFormat::FunctionGemma(FunctionGemmaHandler));
    }
    if template.contains("<tool_call>") || template.contains("</tool_call>") {
        debug!("Detected Qwen3 format from template markers");
        return Ok(ToolFormat::Qwen3(Qwen3Handler));
    }
    if template.contains("[TOOL_CALLS]") {
        debug!("Detected Ministral3 format from template markers");
        return Ok(ToolFormat::Ministral3(Ministral3Handler));
    }
    if template.contains("<|tool_call|>")
        || template.contains("<|/tool_call|>")
        || template.contains("<|tool|>")
        || template.contains("<|/tool|>")
    {
        debug!("Detected Phi-4-mini format from template markers");
        return Ok(ToolFormat::Phi4Mini(Phi4MiniHandler));
    }

    // Fall back to model name
    if let Ok(name) = model.meta_val_str("general.name") {
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
