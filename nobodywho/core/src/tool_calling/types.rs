use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

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
    /// Create a new tool directly. Consider using [`ToolBuilder`] for a more ergonomic API.
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
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("Tool", 2)?;
        state.serialize_field("type", "function")?;
        state.serialize_field(
            "function",
            &json!({
                "name": self.name,
                "description": self.description,
                "parameters": self.json_schema,
            }),
        )?;
        state.end()
    }
}

/// A tool call extracted from LLM output.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
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

    #[error("JSON schema parse error: {0}")]
    JsonSchemaParseError(#[from] gbnf::json::JsonSchemaParseError),
}
