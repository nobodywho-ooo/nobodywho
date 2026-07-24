//! Tool calling support for LLMs with different formats.
//!
//! This module provides abstractions for handling tool calling across different LLM formats.
//! Currently supported formats:
//! - Qwen3: `<tool_call>{"name": "...", "arguments": {...}}</tool_call>`
//! - Qwen3.5/3.6: `<tool_call><function=name><parameter=k>v</parameter>...</function></tool_call>`
//! - FunctionGemma: `<start_function_call>call:name{param:<escape>val<escape>}<end_function_call>`
//! - Gemma4: `<|tool_call>call:name{key:<|"|>val<|"|>}<tool_call|>`
//! - Ministral3: `[TOOL_CALLS][{"name": "...", "arguments": {...}}]`
//! - LFM2: `<|tool_call_start|>[name(key=value, ...)]<|tool_call_end|>`

mod functiongemma;
mod gemma4;
mod lfm2;
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
pub use lfm2::Lfm2Handler;
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

/// Project a JSON schema down to the keys we support, dropping everything else.
/// llguidance's `%json` compiler rejects keys it hasn't implemented (e.g.
/// `uniqueItems`) rather than ignoring them, so anything we don't explicitly
/// keep must go. Recurses only into schema-valued positions; `const`/`enum`/
/// `required` hold literal data and are copied verbatim.
pub(crate) fn project_supported_schema(schema: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    let Some(obj) = schema.as_object() else {
        // Bool schemas (`items: false`, `additionalProperties: false`) and any
        // scalar pass through unchanged.
        return schema.clone();
    };
    let mut out = serde_json::Map::new();
    for (k, v) in obj {
        match k.as_str() {
            // single nested schema (may also be a bool)
            "items" | "additionalProperties" | "not" | "contains" => {
                out.insert(k.clone(), project_supported_schema(v));
            }
            // array of schemas
            "oneOf" | "anyOf" | "allOf" | "prefixItems" => {
                if let Some(arr) = v.as_array() {
                    out.insert(
                        k.clone(),
                        Value::Array(arr.iter().map(project_supported_schema).collect()),
                    );
                }
            }
            // map of schemas
            "properties" | "$defs" | "definitions" | "patternProperties" => {
                if let Some(m) = v.as_object() {
                    let mm = m
                        .iter()
                        .map(|(pk, pv)| (pk.clone(), project_supported_schema(pv)))
                        .collect();
                    out.insert(k.clone(), Value::Object(mm));
                }
            }
            // data / annotations kept verbatim (NOT recursed into)
            "type" | "required" | "const" | "enum" | "$ref" | "description" | "title"
            | "format" => {
                out.insert(k.clone(), v.clone());
            }
            // everything else (uniqueItems, examples, $schema, ...) dropped
            _ => {}
        }
    }
    Value::Object(out)
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
            json_schema: project_supported_schema(&json_schema),
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

    #[error("Lama.cpp failed fetching chat template from the model file. This is likely because you're using an older GGUF file, which might not include a chat template. For example, this is the case for most LLaMA2-based GGUF files. Try using a more recent GGUF model file. {0}")]
    ChatTemplateError(#[from] llama_cpp_2::ChatTemplateError),
}

// ============================================================================
// Trait & Format Enum
// ============================================================================

/// Escape a string for embedding inside a Lark double-quoted string literal.
///
/// Escape user-controlled input for splicing into a double-quoted Lark literal.
/// Besides `"` and `\`, raw newline/tab/carriage-return make the literal
/// malformed, so map them to their C-style escapes too. `\\` is replaced first
/// so the backslashes introduced below are not re-escaped.
pub(crate) fn escape_lark_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Render a tool-call delimiter for embedding in a Lark grammar.
///
/// A delimiter that is a single **control token** (e.g. Ministral's
/// `[TOOL_CALLS]`, LFM2's `<|tool_call_start|>`) is referenced as a Lark special
/// token by id — `<[id]>` — so it matches the single control token the model
/// emits. A quoted literal would instead match the token's text bytes, which
/// never correspond to the control token. Ordinary text delimiters are emitted
/// as an escaped quoted literal. With `model = None` (structure-only tests) the
/// literal form is always used.
pub(crate) fn lark_delimiter(model: Option<&LlamaModel>, s: &str) -> String {
    if let Some(model) = model {
        if let Ok(tokens) = model.str_to_token(s, llama_cpp_2::model::AddBos::Never) {
            if tokens.len() == 1 {
                let tok = tokens[0];
                // A control token has no plaintext rendering: `special=false`
                // yields empty bytes or errors. Anything else is ordinary text.
                let is_control = model
                    .token_to_piece_bytes(tok, 32, false, None)
                    .map(|b| b.is_empty())
                    .unwrap_or(true);
                if is_control {
                    return format!("<[{}]>", tok.0);
                }
            }
        }
    }
    format!("\"{}\"", escape_lark_string(s))
}

/// Map any non-alphanumeric character to `_` for use in Lark rule names.
///
/// Tool/property names come from user-controlled input and are spliced into
/// generated Lark rule names, which only allow a restricted identifier
/// charset.
pub(crate) fn sanitize_lark(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// Trait for handling different tool calling formats.
pub trait ToolFormatHandler {
    /// Returns the token that begins a tool call (e.g., "<tool_call>")
    fn begin_token(&self) -> &str;

    /// Returns the token that ends a tool call (e.g., "</tool_call>")
    fn end_token(&self) -> &str;

    /// Extracts tool calls from the given text.
    fn extract_tool_calls(&self, input: &str) -> Option<Vec<ToolCall>>;

    /// Build a Lark grammar describing the tool call shape, for use with
    /// [`crate::sampler::llguidance_sampler`].
    ///
    /// The grammar starts at the tool-call body — i.e., its entry rule
    /// includes the begin token as its first literal. It does **not** include
    /// a lazy preamble: Lark/llguidance has no equivalent of llama.cpp's
    /// `grammar_lazy` external trigger words, and a `[suffix=...]` preamble
    /// would block EOS while waiting for the trigger to appear. Optional
    /// activation is instead handled by the chat layer, which only inserts
    /// this grammar into the sampler chain after detecting the begin token
    /// in the streamed output.
    fn to_lark(
        &self,
        tools: &[Tool],
        model: Option<&LlamaModel>,
    ) -> Result<String, ToolFormatError>;

    /// Vocabulary hints that speed up grammar-constrained token selection.
    ///
    /// Each regex describes a set of tokens that are commonly allowed at some
    /// position in this format. llguidance pre-computes a bitmask for each
    /// pattern at startup. At generation time, when every valid token at the
    /// current grammar position matches a pattern, llguidance uses the bitmask
    /// directly instead of scanning the full vocabulary — cutting per-token
    /// constraint cost significantly on large vocabularies.
    ///
    /// The default is a JSON string-value body regex (excludes `"`, `\`,
    /// control chars), suitable for JSON-formatted tool calls (e.g. Qwen3).
    /// Handlers whose format is not JSON should override this with patterns
    /// matched to their actual delimiter structure (see `FunctionGemmaHandler`
    /// for an example with `[^<>{},:]+`).
    fn slice_regexes(&self) -> Vec<String> {
        vec![r#"[^"\\\x00-\x1F\x7F]+"#.to_string()]
    }
}

/// Enum representing different tool calling formats.
#[derive(Debug, Clone)]
pub enum ToolFormat {
    Qwen3(Qwen3Handler),
    Qwen35_36(Qwen35_36Handler),
    FunctionGemma(FunctionGemmaHandler),
    Gemma4(Gemma4Handler),
    Ministral3(Ministral3Handler),
    Lfm2(Lfm2Handler),
}

impl ToolFormat {
    pub fn handler(&self) -> &dyn ToolFormatHandler {
        match self {
            ToolFormat::Qwen3(h) => h,
            ToolFormat::Qwen35_36(h) => h,
            ToolFormat::FunctionGemma(h) => h,
            ToolFormat::Gemma4(h) => h,
            ToolFormat::Ministral3(h) => h,
            ToolFormat::Lfm2(h) => h,
        }
    }

    pub fn begin_token(&self) -> &str {
        self.handler().begin_token()
    }

    pub fn end_token(&self) -> &str {
        self.handler().end_token()
    }

    pub fn to_lark(
        &self,
        tools: &[Tool],
        model: Option<&LlamaModel>,
    ) -> Result<String, ToolFormatError> {
        self.handler().to_lark(tools, model)
    }

    pub fn extract_tool_calls(&self, input: &str) -> Option<Vec<ToolCall>> {
        self.handler().extract_tool_calls(input)
    }

    pub fn slice_regexes(&self) -> Vec<String> {
        self.handler().slice_regexes()
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

    // Check for LFM2 markers
    if template_str.contains("<|tool_call_start|>")
        || template_str.contains("<|tool_list_start|>")
        || template_str.contains("<|tool_response_start|>")
    {
        debug!("Detected LFM2 format from template markers");
        return Ok(ToolFormat::Lfm2(Lfm2Handler));
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
        if arch_lower.starts_with("lfm") {
            debug!("Detected LFM2 format from architecture");
            return Ok(ToolFormat::Lfm2(Lfm2Handler));
        }
    }

    if let Ok(name) = model.meta_val_str("general.name") {
        debug!(model_name = %name, "Checking model name for format hints");

        let name_lower = name.to_lowercase();
        if name_lower.contains("lfm") {
            debug!("Detected LFM2 format from model name");
            return Ok(ToolFormat::Lfm2(Lfm2Handler));
        }

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

    fn weather_tool() -> Tool {
        Tool {
            name: "get_weather".to_string(),
            description: "Get weather".to_string(),
            json_schema: json!({
                "type": "object",
                "properties": { "city": {"type": "string"} },
                "required": ["city"]
            }),
            function: Arc::new(|_| String::new()),
        }
    }

    #[test]
    fn tool_new_projects_unsupported_schema_keys() {
        // A `set`-typed parameter carries `uniqueItems`, which llguidance's
        // `%json` compiler rejects. `Tool::new` must strip it (and any other
        // unsupported key) while preserving the structural schema, so the
        // per-format grammars compile.
        let tool = Tool::new(
            "set_intersection",
            "intersect two sets",
            json!({
                "type": "object",
                "properties": {
                    "set1": {
                        "type": "array",
                        "items": {"type": "integer"},
                        "uniqueItems": true
                    },
                    // `const` holds literal data, not a schema: a key that
                    // collides with a dropped keyword must survive untouched.
                    "mode": {"const": {"uniqueItems": 5}}
                },
                "required": ["set1"]
            }),
            Arc::new(|_| String::new()),
        );

        let schema_str = serde_json::to_string(&tool.json_schema).unwrap();

        // The unsupported keyword is gone from every schema position...
        assert!(
            !schema_str.contains("\"uniqueItems\":true"),
            "uniqueItems keyword should be stripped: {schema_str}"
        );
        // ...but structural keys survive.
        let props = &tool.json_schema["properties"];
        assert_eq!(props["set1"]["type"], "array");
        assert_eq!(props["set1"]["items"]["type"], "integer");
        assert_eq!(tool.json_schema["required"][0], "set1");
        // ...and literal `const` data is NOT filtered (recursion stops at data).
        assert_eq!(props["mode"]["const"]["uniqueItems"], 5);

        // The projected schema compiles into a `%json`-embedding grammar.
        ToolFormat::Qwen3(Qwen3Handler)
            .to_lark(&[tool], None)
            .expect("projected schema should produce valid lark");
    }

    #[test]
    fn escape_lark_string_escapes_quotes_backslashes_and_control_chars() {
        assert_eq!(escape_lark_string("a\"b\\c"), "a\\\"b\\\\c");
        assert_eq!(escape_lark_string("l1\nl2\tx\r"), "l1\\nl2\\tx\\r");
        // A literal backslash-n in the input must not collide with the newline
        // escape: `\\` is replaced first, so `\n` (two chars) becomes `\\n`.
        assert_eq!(escape_lark_string("a\\nb"), "a\\\\nb");
    }

    #[test]
    fn to_lark_produces_valid_lark_for_all_handlers() {
        // For each handler, to_lark() should produce a Lark grammar starting
        // with the `%llguidance` header and a `start:` rule whose body
        // contains a reference to the format's begin token (either as a Lark
        // string literal or as a `<...>` special-token reference, depending
        // on how the per-handler grammar represents it).
        let tool = weather_tool();
        let cases: &[(ToolFormat, &str)] = &[
            (ToolFormat::Qwen3(Qwen3Handler), "<tool_call>"),
            (ToolFormat::Qwen35_36(Qwen35_36Handler), "<tool_call>"),
            (
                ToolFormat::FunctionGemma(FunctionGemmaHandler),
                "<start_function_call>",
            ),
            (ToolFormat::Gemma4(Gemma4Handler), "<|tool_call>"),
            (ToolFormat::Ministral3(Ministral3Handler), "[TOOL_CALLS]"),
            (ToolFormat::Lfm2(Lfm2Handler), "<|tool_call_start|>"),
        ];

        for (fmt, trigger) in cases {
            let lark = fmt
                .to_lark(&[tool.clone()], None)
                .unwrap_or_else(|e| panic!("to_lark failed for {:?}: {}", fmt, e));
            assert!(
                lark.starts_with("%llguidance {}"),
                "{:?}: missing llguidance header:\n{}",
                fmt,
                lark
            );
            assert!(
                lark.contains("\nstart:"),
                "{:?}: missing start: rule:\n{}",
                fmt,
                lark
            );
            // The begin token should appear somewhere in the grammar body —
            // either as a literal (`"..."`) or as a special-token reference
            // (`<...>`). For literal handlers we check the quoted form; for
            // Gemma4 (which uses TokenRef::ByString) the converter emits the
            // bare `<...>` form.
            let token_in_lark =
                lark.contains(&format!("\"{}\"", trigger)) || lark.contains(trigger);
            assert!(
                token_in_lark,
                "{:?}: trigger {:?} not found in grammar:\n{}",
                fmt, trigger, lark
            );
        }
    }

    /// Does the model's tokenizer + `grammar` accept `input` as a COMPLETE
    /// match? A real tokenizer is required: the free-form-string rules use
    /// multi-character `suffix` stops, which a single-byte tokenizer can't
    /// resolve. True only if every token is consumed and the parser ends
    /// accepting.
    /// Regression: LFM2's tool-call delimiters `<|tool_call_start|>` /
    /// `<|tool_call_end|>` are control tokens, which the model emits as single
    /// special tokens. The grammar must reference them as Lark special tokens
    /// (`<...>`), not quoted literals — otherwise the special token (0xFF-marked
    /// in the toktrie) fails the literal-bytes rule the moment the grammar
    /// activates (`byte 'ÿ' fails parse`). Self-gated: skips unless the loaded
    /// TEST_MODEL actually has these control tokens.
    #[test]
    fn lfm2_control_token_delimiters_accepted() {
        use llguidance::toktrie::InferenceCapabilities;
        use llguidance::{api::TopLevelGrammar, Matcher, ParserFactory};

        let model = crate::test_utils::load_test_model();
        let tok_env =
            llama_cpp_2::sampling::LlamaSampler::llguidance_tok_env(&model.language_model);
        if tok_env.tok_trie().get_special_token("<|tool_call_start|>").is_none() {
            eprintln!("skipping: TEST_MODEL is not an LFM2 model (no <|tool_call_start|> token)");
            return;
        }

        let grammar = ToolFormat::Lfm2(Lfm2Handler)
            .to_lark(&[weather_tool()], Some(&model.language_model))
            .unwrap();

        let factory = ParserFactory::new(&tok_env, InferenceCapabilities::default(), &[])
            .expect("build ParserFactory");
        let grm =
            TopLevelGrammar::from_tagged_str("lark", &grammar).expect("parse Lark grammar");
        let parser = factory.create_parser(grm).expect("create parser");
        let mut matcher = Matcher::new(Ok(parser));

        // Tokenize the way real generation does (parse_special=true) so the
        // delimiters become the single control tokens, not spelled-out text —
        // this is the case the toktrie's `tokenize_special` can't reproduce.
        let tokens: Vec<u32> = model
            .language_model
            .str_to_token(
                "<|tool_call_start|>[get_weather(city=\"Paris\")]<|tool_call_end|>",
                llama_cpp_2::model::AddBos::Never,
            )
            .unwrap()
            .iter()
            .map(|t| t.0 as u32)
            .collect();
        let consumed = matcher.try_consume_tokens(&tokens).unwrap_or(0);
        assert!(
            consumed == tokens.len() && matcher.is_accepting().unwrap_or(false),
            "LFM2 call with control-token delimiters should be accepted \
             (consumed {consumed}/{}):\n{grammar}",
            tokens.len()
        );
    }

    /// Regression: Ministral3's `[TOOL_CALLS]` / `[ARGS]` delimiters are control
    /// tokens, so the grammar must reference them by id (`<[id]>`); as quoted
    /// literals the grammar fails to constrain the control token the model emits.
    /// Self-gated: skips unless the loaded TEST_MODEL has these control tokens.
    #[test]
    fn ministral3_control_token_delimiters_accepted() {
        use llguidance::toktrie::InferenceCapabilities;
        use llguidance::{api::TopLevelGrammar, Matcher, ParserFactory};

        let model = crate::test_utils::load_test_model();
        let tok_env =
            llama_cpp_2::sampling::LlamaSampler::llguidance_tok_env(&model.language_model);
        if tok_env.tok_trie().get_special_token("[TOOL_CALLS]").is_none() {
            eprintln!("skipping: TEST_MODEL is not a Ministral model (no [TOOL_CALLS] token)");
            return;
        }

        let grammar = ToolFormat::Ministral3(Ministral3Handler)
            .to_lark(&[weather_tool()], Some(&model.language_model))
            .unwrap();

        let factory = ParserFactory::new(&tok_env, InferenceCapabilities::default(), &[])
            .expect("build ParserFactory");
        let grm =
            TopLevelGrammar::from_tagged_str("lark", &grammar).expect("parse Lark grammar");
        let parser = factory.create_parser(grm).expect("create parser");
        let mut matcher = Matcher::new(Ok(parser));

        let tokens: Vec<u32> = model
            .language_model
            .str_to_token(
                "[TOOL_CALLS]get_weather[ARGS]{\"city\": \"Paris\"}",
                llama_cpp_2::model::AddBos::Never,
            )
            .unwrap()
            .iter()
            .map(|t| t.0 as u32)
            .collect();
        let consumed = matcher.try_consume_tokens(&tokens).unwrap_or(0);
        assert!(
            consumed == tokens.len() && matcher.is_accepting().unwrap_or(false),
            "Ministral call with control-token delimiters should be accepted \
             (consumed {consumed}/{}):\n{grammar}",
            tokens.len()
        );
    }

    fn grammar_accepts_tok(model: &crate::llm::Model, grammar: &str, input: &str) -> bool {
        use llguidance::toktrie::InferenceCapabilities;
        use llguidance::{api::TopLevelGrammar, Matcher, ParserFactory};

        let tok_env =
            llama_cpp_2::sampling::LlamaSampler::llguidance_tok_env(&model.language_model);
        let factory = ParserFactory::new(&tok_env, InferenceCapabilities::default(), &[])
            .expect("build ParserFactory");
        let grm = TopLevelGrammar::from_tagged_str("lark", grammar).expect("parse Lark grammar");
        let parser = factory.create_parser(grm).expect("create parser");
        let mut matcher = Matcher::new(Ok(parser));

        let tokens = tok_env.tokenize_special(input);
        let consumed = matcher.try_consume_tokens(&tokens).unwrap_or(0);
        consumed == tokens.len() && matcher.is_accepting().unwrap_or(false)
    }

    /// Regression: Gemma4 free-form string values must allow a literal `<`
    /// (e.g. "count < 5", HTML, generics). The old body rule `/[^<]*/` banned
    /// every `<`, not just the closing `<|"|>` delimiter, so such values could
    /// not be generated; the `gemmafour_strbody[suffix=...]` rule fixes it.
    /// Requires a Gemma4 GGUF — set `GEMMA4_MODEL`; skipped otherwise.
    #[test]
    fn gemma4_string_value_allows_left_angle_bracket() {
        let Ok(path) = std::env::var("GEMMA4_MODEL") else {
            eprintln!("skipping: set GEMMA4_MODEL to a Gemma4 GGUF to run this test");
            return;
        };
        let model = crate::llm::get_model(&path, true, None, None)
            .unwrap_or_else(|e| panic!("failed to load Gemma4 model from {path}: {e:?}"));
        let grammar = ToolFormat::Gemma4(Gemma4Handler)
            .to_lark(&[weather_tool()], Some(&model.language_model))
            .unwrap();

        // Baseline: a plain value is accepted.
        assert!(
            grammar_accepts_tok(
                &model,
                &grammar,
                "<|tool_call>call:get_weather{city:<|\"|>hello<|\"|>}<tool_call|>"
            ),
            "plain string value should be accepted:\n{grammar}"
        );
        // The regression: a value containing '<' is valid content.
        assert!(
            grammar_accepts_tok(
                &model,
                &grammar,
                "<|tool_call>call:get_weather{city:<|\"|>count < 5<|\"|>}<tool_call|>"
            ),
            "string value containing '<' should be accepted:\n{grammar}"
        );
    }

    /// Regression: Qwen3.5/3.6 free-form string values must be able to end in a
    /// trailing newline. The old body rule `/([^\n]|\n[^<])*/` could not match a
    /// value ending in `\n` before the `\n</parameter>\n` terminator (no valid
    /// split of the bytes); the `..._body[suffix=...]` rule fixes it. Uses the
    /// TEST_MODEL tokenizer to validate the generated grammar's byte language.
    #[test]
    fn qwen35_36_string_value_allows_trailing_newline() {
        let model = crate::test_utils::load_test_model();
        let grammar = ToolFormat::Qwen35_36(Qwen35_36Handler)
            .to_lark(&[weather_tool()], Some(&model.language_model))
            .unwrap();

        // Baseline: a plain value is accepted.
        assert!(
            grammar_accepts_tok(
                &model,
                &grammar,
                "<tool_call>\n<function=get_weather>\n<parameter=city>\nsunny\n</parameter>\n</function>\n</tool_call>"
            ),
            "plain string value should be accepted:\n{grammar}"
        );
        // The regression: value "sunny\n" ends in a newline.
        assert!(
            grammar_accepts_tok(
                &model,
                &grammar,
                "<tool_call>\n<function=get_weather>\n<parameter=city>\nsunny\n\n</parameter>\n</function>\n</tool_call>"
            ),
            "string value ending in a newline should be accepted:\n{grammar}"
        );
    }

    /// Regression: enum string values containing control characters (here a
    /// newline) must be escaped when spliced into Lark literals, or the grammar
    /// is malformed and `create_parser` fails at worker init. See
    /// [`escape_lark_string`]. Uses the TEST_MODEL tokenizer.
    #[test]
    fn qwen35_36_enum_value_with_newline_produces_valid_grammar() {
        let model = crate::test_utils::load_test_model();
        let tool = Tool::new(
            "set_mode",
            "set the mode",
            json!({
                "type": "object",
                "properties": {
                    "mode": {"type": "string", "enum": ["plain", "with\nnewline"]}
                },
                "required": ["mode"]
            }),
            Arc::new(|_| String::new()),
        );
        let grammar = ToolFormat::Qwen35_36(Qwen35_36Handler)
            .to_lark(&[tool], Some(&model.language_model))
            .unwrap();

        // Accepting the escaped-newline variant also proves the grammar parsed:
        // a raw newline in the literal would make `create_parser` fail.
        assert!(
            grammar_accepts_tok(
                &model,
                &grammar,
                "<tool_call>\n<function=set_mode>\n<parameter=mode>\nwith\nnewline\n</parameter>\n</function>\n</tool_call>"
            ),
            "enum value with an escaped newline should be accepted:\n{grammar}"
        );
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
            ToolFormat::Lfm2(_) => "Lfm2",
        };
        eprintln!("detected handler     = {variant}");
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
