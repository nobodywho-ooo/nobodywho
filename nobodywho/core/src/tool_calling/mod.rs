//! Tool calling support for LLMs with different formats.
//!
//! This module provides abstractions for handling tool calling across different LLM formats.
//! Currently supported formats:
//! - Qwen3: `<tool_call>{"name": "...", "arguments": {...}}</tool_call>`
//! - Qwen3.5/3.6: `<tool_call><function=name><parameter=k>v</parameter>...</function></tool_call>`
//! - FunctionGemma: `<start_function_call>call:name{param:<escape>val<escape>}<end_function_call>`
//! - Gemma4: `<|tool_call>call:name{key:<|"|>val<|"|>}<tool_call|>`
//! - Ministral3: `[TOOL_CALLS][{"name": "...", "arguments": {...}}]`
//! - Llama-3.x: `<|python_tag|>{"name"|"function": "...", "parameters": {...}}<|eot_id|>` (prefix optional in extraction; model often omits it)

mod functiongemma;
mod gemma4;
mod llama32;
mod ministral3;
mod qwen3;
mod qwen35_36;

use bashkit::{ExecutionLimits, InMemoryFs};
use llama_cpp_2::model::LlamaModel;
use monty::{LimitedTracker, MontyRun, PrintWriter, ResourceLimits};
use serde::{ser::Serializer, Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use tracing::debug;

pub use functiongemma::FunctionGemmaHandler;
pub use gemma4::Gemma4Handler;
pub use llama32::Llama32Handler;
pub use ministral3::Ministral3Handler;
pub use qwen3::Qwen3Handler;
pub use qwen35_36::Qwen35_36Handler;

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
//
// IMPORTANT: emit keys in EXPLICIT INSERTION ORDER (`type` before `function`,
// inner `name` -> `description` -> `parameters`). The default `serde_json::json!`
// path materialises a `Value::Object` backed by `BTreeMap`, which re-sorts keys
// alphabetically. LFM2.5-350M is sensitive to that order — when fed
// `{"function":{...},"type":"function"}` (alphabetical), it hallucinates extra
// Pythonic kwargs (`parameters=`, `type=`) in the tool call. Using `serialize_map`
// directly with nested helpers preserves the canonical OpenAI key order and
// avoids forcing the `serde_json/preserve_order` cargo feature globally.
impl Serialize for Tool {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;

        struct OrderedFunction<'a> {
            name: &'a str,
            description: &'a str,
            parameters: &'a serde_json::Value,
        }

        impl Serialize for OrderedFunction<'_> {
            fn serialize<S2>(&self, serializer: S2) -> Result<S2::Ok, S2::Error>
            where
                S2: Serializer,
            {
                let mut m = serializer.serialize_map(Some(3))?;
                m.serialize_entry("name", self.name)?;
                m.serialize_entry("description", self.description)?;
                m.serialize_entry("parameters", self.parameters)?;
                m.end()
            }
        }

        let function = OrderedFunction {
            name: &self.name,
            description: &self.description,
            parameters: &self.json_schema,
        };

        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("type", "function")?;
        map.serialize_entry("function", &function)?;
        map.end()
    }
}

/// A tool call extracted from LLM output.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

// Serialize tool calls according to https://huggingface.co/blog/unified-tool-use
// in canonical OpenAI key order (`type`, `function`, then within `function`:
// `name`, `arguments`). Implemented via `serialize_map` rather than
// `serde_json::json!({...}).serialize(serializer)` because the latter routes
// through `serde_json::Value`'s default `BTreeMap` representation, which sorts
// keys alphabetically (`{"function":{"arguments":...,"name":...},"type":...}`)
// unless serde_json is built with the `preserve_order` feature — which is OFF
// for the core/flutter/uniffi crates and which we cannot rely on downstream.
// Same rationale as `Serialize for Tool` above.
impl Serialize for ToolCall {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;

        struct OrderedFunction<'a> {
            name: &'a str,
            arguments: &'a serde_json::Value,
        }

        impl Serialize for OrderedFunction<'_> {
            fn serialize<S2>(&self, serializer: S2) -> Result<S2::Ok, S2::Error>
            where
                S2: Serializer,
            {
                let mut m = serializer.serialize_map(Some(2))?;
                m.serialize_entry("name", self.name)?;
                m.serialize_entry("arguments", self.arguments)?;
                m.end()
            }
        }

        let function = OrderedFunction {
            name: &self.name,
            arguments: &self.arguments,
        };

        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("type", "function")?;
        map.serialize_entry("function", &function)?;
        map.end()
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

    /// Optional list of regex patterns that activate the tool-call grammar in
    /// the sampler. Returning `Some(_)` selects llama.cpp's pattern-based lazy
    /// grammar (`grammar_lazy_patterns`); returning `None` falls back to the
    /// single-token trigger keyed on `begin_token()`. Pattern triggers are
    /// needed by handlers whose chat templates do not deterministically force
    /// the begin token on every tool-call turn — Llama-3.x is the canonical
    /// case: `<|python_tag|>` is emitted on the first tool-call turn but
    /// omitted on post-tool-response turns, so the model emits raw `{...}`
    /// JSON and a single-token trigger never re-fires.
    fn grammar_trigger_patterns(&self) -> Option<Vec<String>> {
        None
    }

    /// Whether the model may emit additional tool calls after the first
    /// dispatch in a single user turn. Default: `true` — the chat loop keeps
    /// the tool-call grammar active and re-extracts on every regeneration.
    /// Override to `false` for families that empirically loop on the same
    /// tool dispatch instead of producing a final text response (Llama-3.x
    /// is the canonical case; cf. llama.cpp `common/chat.cpp:1626`
    /// `auto max_calls = 1; // parallel toolcalls are not supported` and
    /// vLLM's "parallel tool calls are not supported for Llama 3"). When
    /// `false`, the chat loop dispatches the first tool call, then forces
    /// the next regeneration to use the base sampler (no tool grammar) so
    /// the model produces a plain-text answer to the user, and exits the
    /// dispatch loop.
    fn allow_repeated_calls(&self) -> bool {
        true
    }

    /// Generates a GBNF grammar for constrained sampling of tool calls.
    fn generate_grammar(&self, tools: &[Tool]) -> Result<gbnf::GbnfGrammar, ToolFormatError>;

    /// Extracts tool calls from the given text.
    fn extract_tool_calls(&self, input: &str) -> Option<Vec<ToolCall>>;
}

/// Enum representing different tool calling formats.
#[derive(Debug, Clone)]
pub enum ToolFormat {
    Qwen3(Qwen3Handler),
    Qwen35_36(Qwen35_36Handler),
    FunctionGemma(FunctionGemmaHandler),
    Gemma4(Gemma4Handler),
    Ministral3(Ministral3Handler),
    Llama32(Llama32Handler),
}

impl ToolFormat {
    pub fn handler(&self) -> &dyn ToolFormatHandler {
        match self {
            ToolFormat::Qwen3(h) => h,
            ToolFormat::Qwen35_36(h) => h,
            ToolFormat::FunctionGemma(h) => h,
            ToolFormat::Gemma4(h) => h,
            ToolFormat::Ministral3(h) => h,
            ToolFormat::Llama32(h) => h,
        }
    }

    pub fn begin_token(&self) -> &str {
        self.handler().begin_token()
    }

    pub fn end_token(&self) -> &str {
        self.handler().end_token()
    }

    pub fn grammar_trigger_patterns(&self) -> Option<Vec<String>> {
        self.handler().grammar_trigger_patterns()
    }

    pub fn allow_repeated_calls(&self) -> bool {
        self.handler().allow_repeated_calls()
    }

    pub fn generate_grammar(&self, tools: &[Tool]) -> Result<gbnf::GbnfGrammar, ToolFormatError> {
        self.handler().generate_grammar(tools)
    }

    pub fn extract_tool_calls(&self, input: &str) -> Option<Vec<ToolCall>> {
        self.handler().extract_tool_calls(input)
    }
}

fn is_qwen35_36_architecture(arch: &str) -> bool {
    let arch = arch.to_lowercase();
    arch.starts_with("qwen35")
        || arch.starts_with("qwen36")
        || arch.contains("qwen3.5")
        || arch.contains("qwen3.6")
}

fn is_qwen35_36_name(name: &str) -> bool {
    let name = name.to_lowercase();
    [
        "qwen3.5", "qwen3.6", "qwen 3.5", "qwen 3.6", "qwen-3.5", "qwen-3.6", "qwen35", "qwen36",
    ]
    .iter()
    .any(|needle| name.contains(needle))
}

fn is_qwen3_name(name: &str) -> bool {
    let name = name.to_lowercase();
    name.contains("qwen3") || name.contains("qwen 3") || name.contains("qwen-3")
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

    // Check for Gemma4 markers (must be before Qwen since both contain "tool_call")
    if template_str.contains("<|tool_call>") || template_str.contains("<tool_call|>") {
        debug!("Detected Gemma4 format from template markers");
        return Ok(ToolFormat::Gemma4(Gemma4Handler));
    }

    // Qwen 3.5/3.6
    let has_qwen_call =
        template_str.contains("<tool_call>") || template_str.contains("</tool_call>");
    if has_qwen_call {
        if template_str.contains("<function=") {
            debug!("Detected Qwen3.5/3.6 format from template markers");
            return Ok(ToolFormat::Qwen35_36(Qwen35_36Handler));
        }
        debug!("Detected Qwen3 format from template markers");
        return Ok(ToolFormat::Qwen3(Qwen3Handler));
    }

    // Check for Ministral3 markers
    if template_str.contains("[TOOL_CALLS]") {
        debug!("Detected Ministral3 format from template markers");
        return Ok(ToolFormat::Ministral3(Ministral3Handler));
    }

    // Check for Llama-3.x markers. Both signals are unique to Llama-3.1+
    // tool-use templates; Llama-2 templates have neither.
    if template_str.contains("Environment: ipython") || template_str.contains("<|python_tag|>") {
        debug!("Detected Llama-3.x format from template markers");
        return Ok(ToolFormat::Llama32(Llama32Handler));
    }

    // Fall back to model metadata.
    if let Ok(arch) = model.meta_val_str("general.architecture") {
        debug!(architecture = %arch, "Checking model architecture for format hints");
        let arch_lower = arch.to_lowercase();
        if is_qwen35_36_architecture(&arch_lower) {
            debug!("Detected Qwen3.5/3.6 format from architecture");
            return Ok(ToolFormat::Qwen35_36(Qwen35_36Handler));
        }
        if arch_lower.starts_with("qwen3") {
            debug!("Detected Qwen3 format from architecture");
            return Ok(ToolFormat::Qwen3(Qwen3Handler));
        }
        // Llama-3.x architecture (after Qwen so Qwen-llama derivatives still
        // match Qwen first). Llama-2 GGUFs have no tool-use template; the
        // template marker check above already would have rejected them, so
        // hitting this branch with arch="llama" implies a 3.x family.
        if arch_lower.starts_with("llama") {
            debug!("Detected Llama-3.x format from architecture");
            return Ok(ToolFormat::Llama32(Llama32Handler));
        }
    }

    if let Ok(name) = model.meta_val_str("general.name") {
        debug!(model_name = %name, "Checking model name for format hints");

        let name_lower = name.to_lowercase();
        if name_lower.contains("functiongemma") || name_lower.contains("function-gemma") {
            debug!("Detected FunctionGemma format from model name");
            return Ok(ToolFormat::FunctionGemma(FunctionGemmaHandler));
        }

        if name_lower.contains("gemma-4") || name_lower.contains("gemma4") {
            debug!("Detected Gemma4 format from model name");
            return Ok(ToolFormat::Gemma4(Gemma4Handler));
        }

        if is_qwen35_36_name(&name_lower) {
            debug!("Detected Qwen3.5/3.6 format from model name");
            return Ok(ToolFormat::Qwen35_36(Qwen35_36Handler));
        }

        if is_qwen3_name(&name_lower) || name_lower.contains("qwen") {
            debug!("Detected Qwen3 format from model name");
            return Ok(ToolFormat::Qwen3(Qwen3Handler));
        }

        // Llama-3.x by name. Require the "3" suffix to avoid grabbing
        // Llama-2 GGUFs which don't speak tool-use.
        if name_lower.contains("llama-3")
            || name_lower.contains("llama 3")
            || name_lower.contains("llama3")
        {
            debug!("Detected Llama-3.x format from model name");
            return Ok(ToolFormat::Llama32(Llama32Handler));
        }
    }

    Err(ToolFormatError::UnsupportedFormat(
        "Cannot detect tool format from template or model family".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    fn test_qwen3_format() {
        let format = ToolFormat::Qwen3(Qwen3Handler);
        assert_eq!(format.begin_token(), "<tool_call>");
        assert_eq!(format.end_token(), "</tool_call>");
    }

    #[test]
    fn test_llama32_format() {
        let format = ToolFormat::Llama32(Llama32Handler);
        assert_eq!(format.begin_token(), "<|python_tag|>");
        assert_eq!(format.end_token(), "<|eot_id|>");
    }

    #[test]
    fn test_functiongemma_format() {
        let format = ToolFormat::FunctionGemma(FunctionGemmaHandler);
        assert_eq!(format.begin_token(), "<start_function_call>");
        assert_eq!(format.end_token(), "<end_function_call>");
    }

    #[test]
    fn test_tool_serialization() {
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
    fn test_toolcall_serialization_preserves_canonical_key_order() {
        // Regression guard for #519 review (gergelyvagujhelyi): the previous
        // impl used `serde_json::json!({...}).serialize(serializer)`, which
        // routes through `Value`'s default `BTreeMap` and emits keys in
        // alphabetic order (`{"function":{"arguments":...,"name":...},...`)
        // unless serde_json is built with `preserve_order` — OFF for the
        // core/flutter/uniffi crates. Compare against the raw JSON string
        // (NOT a Value) so any future regression to alphabetic ordering is
        // caught even when a Value-equality check would still pass.
        let call = ToolCall {
            name: "circle_area".to_string(),
            arguments: json!({"radius": 5}),
        };
        let serialized = serde_json::to_string(&call).expect("serialize ToolCall");
        assert_eq!(
            serialized,
            r#"{"type":"function","function":{"name":"circle_area","arguments":{"radius":5}}}"#,
            "ToolCall must serialize in canonical OpenAI key order \
             (`type`, `function`, then `name`, `arguments`); got: {serialized}",
        );
    }

    #[test]
    #[ignore = "requires QWEN36_MODEL env var pointing at a Qwen3.6 GGUF"]
    fn diagnose_qwen36_detection() {
        let path = std::env::var("QWEN36_MODEL").expect("set QWEN36_MODEL");
        let model = crate::llm::get_model(&path, false, None, None).expect("load model");

        let name = model
            .language_model
            .meta_val_str("general.name")
            .unwrap_or_else(|_| "<no general.name>".into());
        let arch = model
            .language_model
            .meta_val_str("general.architecture")
            .unwrap_or_else(|_| "<no arch>".into());
        eprintln!("general.name         = {name}");
        eprintln!("general.architecture = {arch}");

        let has_tool_use_tmpl = model.language_model.chat_template(Some("tool_use")).is_ok();
        eprintln!("has tool_use template: {has_tool_use_tmpl}");

        let default_tmpl: String = model
            .language_model
            .chat_template(None)
            .ok()
            .and_then(|t| t.to_string().ok())
            .unwrap_or_default();
        for marker in [
            "<tool_call>",
            "</tool_call>",
            "<|tool_call>",
            "<tool_call|>",
            "<start_function_call>",
            "[TOOL_CALLS]",
        ] {
            eprintln!(
                "default_tmpl contains {marker:>22}: {}",
                default_tmpl.contains(marker)
            );
        }

        let fmt = detect_tool_format(&model.language_model).expect("detect_tool_format failed");
        let variant = match fmt {
            ToolFormat::Qwen3(_) => "Qwen3",
            ToolFormat::Qwen35_36(_) => "Qwen35_36",
            ToolFormat::FunctionGemma(_) => "FunctionGemma",
            ToolFormat::Gemma4(_) => "Gemma4",
            ToolFormat::Ministral3(_) => "Ministral3",
            ToolFormat::Llama32(_) => "Llama32",
        };
        eprintln!("detected handler     = {variant}");
    }

    #[test]
    #[ignore = "requires LLAMA32_MODEL env var pointing at a Llama-3.x GGUF"]
    fn diagnose_llama32_detection() {
        let path = std::env::var("LLAMA32_MODEL").expect("set LLAMA32_MODEL");
        let model = crate::llm::get_model(&path, false, None, None).expect("load model");

        let name = model
            .language_model
            .meta_val_str("general.name")
            .unwrap_or_else(|_| "<no general.name>".into());
        let arch = model
            .language_model
            .meta_val_str("general.architecture")
            .unwrap_or_else(|_| "<no arch>".into());
        eprintln!("general.name         = {name}");
        eprintln!("general.architecture = {arch}");

        let default_tmpl: String = model
            .language_model
            .chat_template(None)
            .ok()
            .and_then(|t| t.to_string().ok())
            .unwrap_or_default();
        for marker in ["Environment: ipython", "<|python_tag|>", "<|eot_id|>"] {
            eprintln!(
                "default_tmpl contains {marker:>22}: {}",
                default_tmpl.contains(marker)
            );
        }

        let fmt = detect_tool_format(&model.language_model).expect("detect_tool_format failed");
        let variant = match fmt {
            ToolFormat::Qwen3(_) => "Qwen3",
            ToolFormat::Qwen35_36(_) => "Qwen35_36",
            ToolFormat::FunctionGemma(_) => "FunctionGemma",
            ToolFormat::Gemma4(_) => "Gemma4",
            ToolFormat::Ministral3(_) => "Ministral3",
            ToolFormat::Llama32(_) => "Llama32",
        };
        eprintln!("detected handler     = {variant}");
        assert_eq!(
            variant, "Llama32",
            "expected Llama32 handler for {path}, got {variant}"
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

    #[test]
    fn test_qwen35_36_name_detection_beats_generic_qwen3() {
        for name in [
            "Qwen3.5-2B-Instruct",
            "Qwen3.6-30B-A3B",
            "Qwen 3.5 coder",
            "Qwen-3.6 reasoning",
            "qwen35",
            "qwen36",
        ] {
            assert!(is_qwen35_36_name(name), "{name} should map to Qwen35_36");
        }

        assert!(!is_qwen35_36_name("Qwen3-8B-Instruct"));
        assert!(is_qwen3_name("Qwen3-8B-Instruct"));
    }

    #[test]
    fn test_qwen35_36_architecture_detection_beats_generic_qwen3() {
        for arch in ["qwen35", "qwen35moe", "qwen36", "qwen3.5", "qwen3.6"] {
            assert!(
                is_qwen35_36_architecture(arch),
                "{arch} should map to Qwen35_36"
            );
        }

        assert!(!is_qwen35_36_architecture("qwen3"));
    }
}
