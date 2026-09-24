//! Tools a model can call, and the calls it makes. How a model writes calls
//! is up to its [`crate::output_format::OutputFormat`].

use bashkit::{ExecutionLimits, InMemoryFs};
use monty::{LimitedTracker, MontyRun, PrintWriter, ResourceLimits};
use serde::{ser::Serializer, Deserialize, Serialize};
use std::{sync::Arc, time::Duration};

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
    fn test_tool_call_serialization() {
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
