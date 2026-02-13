use crate::sampler_config::SamplerConfig;
use crate::tool_calling::Tool;
use llama_cpp_2::model::LlamaModel;
use std::sync::Arc;

use super::handle::{ChatHandle, ChatHandleAsync};

///
/// Configuration for chat sessions.
///
/// This struct groups all the settings needed to initialize a chat worker.
/// Use [`ChatBuilder`] for a more ergonomic way to configure these settings.
pub struct ChatConfig {
    /// Available tools for the model to use.
    pub tools: Vec<Tool>,
    /// Context window size.
    pub n_ctx: u32,
    /// System prompt for the chat session.
    pub system_prompt: String,
    /// Whether to allow thinking mode during inference.
    pub allow_thinking: bool,
    /// Sampler configuration for inference.
    pub sampler_config: SamplerConfig,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            n_ctx: 4096,
            allow_thinking: true,
            system_prompt: String::new(),
            tools: Vec::new(),
            sampler_config: SamplerConfig::default(),
        }
    }
}

/// Builder for creating a [`ChatHandle`] with a fluent API.
///
/// # Example
/// ```
/// use nobodywho::chat::{ChatBuilder};
/// use nobodywho::tool_calling::Tool;
/// use nobodywho::llm;
/// use std::sync::Arc;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let model = llm::get_model("model.gguf", true)?;
///
/// let my_tool = Tool::new(
///     "example".to_string(),
///     "Example tool".to_string(),
///     serde_json::json!({}),
///     Arc::new(|_| "result".to_string())
/// );
///
/// let chat = ChatBuilder::new(model)
///     .with_context_size(4096)
///     .with_system_prompt("You're a helpful assistant")
///     .with_tool(my_tool)
///     .build();
/// # Ok(())
/// # }
/// ```
pub struct ChatBuilder {
    model: Arc<LlamaModel>,
    config: ChatConfig,
}

impl ChatBuilder {
    /// Create a new chat builder with a model.
    pub fn new(model: Arc<LlamaModel>) -> Self {
        Self {
            model,
            config: ChatConfig::default(),
        }
    }

    /// Set the context size for the chat session.
    pub fn with_context_size(mut self, n_ctx: u32) -> Self {
        self.config.n_ctx = n_ctx;
        self
    }

    /// Set the system prompt for the chat session.
    pub fn with_system_prompt<S: Into<String>>(mut self, prompt: S) -> Self {
        self.config.system_prompt = prompt.into();
        self
    }

    /// Add a tool that the model can use.
    pub fn with_tool(mut self, tool: Tool) -> Self {
        self.config.tools.push(tool);
        self
    }

    /// Add multiple tools that the model can use.
    pub fn with_tools(mut self, tools: Vec<Tool>) -> Self {
        self.config.tools.extend(tools);
        self
    }

    /// Allow thinking mode during inference.
    pub fn with_allow_thinking(mut self, allow_thinking: bool) -> Self {
        self.config.allow_thinking = allow_thinking;
        self
    }

    /// Set a custom sampler configuration
    pub fn with_sampler(mut self, sampler: SamplerConfig) -> Self {
        self.config.sampler_config = sampler;
        self
    }

    /// Build a blocking chat handle and start the background worker.
    pub fn build(self) -> ChatHandle {
        ChatHandle::new(self.model, self.config)
    }

    /// Build an async chat handle and start the background worker.
    pub fn build_async(self) -> ChatHandleAsync {
        ChatHandleAsync::new(self.model, self.config)
    }
}
