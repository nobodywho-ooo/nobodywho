use std::sync::Arc;

use napi::bindgen_prelude::*;
use napi_derive::napi;

// ---------- Model ----------

#[napi]
pub struct Model {
    inner: Arc<nobodywho::llm::Model>,
}

#[napi]
impl Model {
    /// Load a GGUF model from disk.
    #[napi(factory)]
    pub async fn load(
        model_path: String,
        use_gpu: bool,
        image_model_path: Option<String>,
    ) -> Result<Self> {
        let model = nobodywho::llm::get_model_async(model_path, use_gpu, image_model_path)
            .await
            .map_err(|e| Error::from_reason(e.to_string()))?;

        Ok(Self {
            inner: Arc::new(model),
        })
    }

    /// Check if a discrete GPU is available.
    #[napi]
    pub fn has_discrete_gpu() -> bool {
        nobodywho::llm::has_discrete_gpu()
    }
}

// ---------- Chat ----------

#[napi]
pub struct Chat {
    inner: nobodywho::chat::ChatHandleAsync,
}

#[napi]
impl Chat {
    /// Create a new chat session.
    #[napi(constructor)]
    pub fn new(
        model: &Model,
        system_prompt: Option<String>,
        context_size: Option<u32>,
    ) -> Self {
        let chat = nobodywho::chat::ChatBuilder::new(Arc::clone(&model.inner))
            .with_context_size(context_size.unwrap_or(4096))
            .with_system_prompt(system_prompt)
            .build_async();

        Self { inner: chat }
    }

    /// Send a message and get a token stream for the response.
    #[napi]
    pub fn ask(&self, message: String) -> TokenStream {
        TokenStream {
            inner: tokio::sync::Mutex::new(self.inner.ask(message)),
        }
    }

    /// Stop the current generation.
    #[napi]
    pub fn stop_generation(&self) {
        self.inner.stop_generation();
    }
}

// ---------- TokenStream ----------

#[napi]
pub struct TokenStream {
    // Mutex needed because napi wraps objects in references with &self,
    // but TokenStreamAsync methods require &mut self.
    inner: tokio::sync::Mutex<nobodywho::chat::TokenStreamAsync>,
}

#[napi]
impl TokenStream {
    /// Get the next token. Returns null when generation is complete.
    #[napi]
    pub async fn next_token(&self) -> Option<String> {
        self.inner.lock().await.next_token().await
    }

    /// Wait for the full response to complete and return it.
    #[napi]
    pub async fn completed(&self) -> Result<String> {
        self.inner
            .lock()
            .await
            .completed()
            .await
            .map_err(|e| Error::from_reason(e.to_string()))
    }
}
