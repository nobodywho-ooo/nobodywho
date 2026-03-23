use std::sync::Arc;

uniffi::setup_scaffolding!();

// ---------- Model ----------

#[derive(uniffi::Object)]
pub struct Model {
    inner: Arc<nobodywho::llm::Model>,
}

#[uniffi::export]
impl Model {
    /// Load a GGUF model from disk.
    #[uniffi::constructor]
    pub async fn load(
        model_path: String,
        use_gpu: bool,
        image_model_path: Option<String>,
    ) -> Result<Arc<Self>, String> {
        let model =
            nobodywho::llm::get_model_async(model_path, use_gpu, image_model_path)
                .await
                .map_err(|e| e.to_string())?;

        Ok(Arc::new(Self {
            inner: Arc::new(model),
        }))
    }
}

/// Check if a discrete GPU is available.
#[uniffi::export]
pub fn has_discrete_gpu() -> bool {
    nobodywho::llm::has_discrete_gpu()
}

// ---------- Chat ----------

#[derive(uniffi::Object)]
pub struct Chat {
    inner: nobodywho::chat::ChatHandleAsync,
}

#[uniffi::export]
impl Chat {
    /// Create a new chat session.
    #[uniffi::constructor]
    pub fn new(model: &Model, system_prompt: Option<String>, context_size: u32) -> Self {
        let chat = nobodywho::chat::ChatBuilder::new(Arc::clone(&model.inner))
            .with_context_size(context_size)
            .with_system_prompt(system_prompt)
            .build_async();

        Self { inner: chat }
    }

    /// Send a message and get a token stream for the response.
    pub fn ask(&self, message: String) -> Arc<TokenStream> {
        Arc::new(TokenStream {
            inner: tokio::sync::Mutex::new(self.inner.ask(message)),
        })
    }

    /// Stop the current generation.
    pub fn stop_generation(&self) {
        self.inner.stop_generation();
    }
}

// ---------- TokenStream ----------

#[derive(uniffi::Object)]
pub struct TokenStream {
    // Mutex needed because UniFFI wraps objects in Arc (giving &self),
    // but TokenStreamAsync methods require &mut self.
    inner: tokio::sync::Mutex<nobodywho::chat::TokenStreamAsync>,
}

#[uniffi::export]
impl TokenStream {
    /// Get the next token. Returns None when generation is complete.
    pub async fn next_token(&self) -> Option<String> {
        self.inner.lock().await.next_token().await
    }

    /// Wait for the full response to complete and return it.
    pub async fn completed(&self) -> Result<String, String> {
        self.inner
            .lock()
            .await
            .completed()
            .await
            .map_err(|e| e.to_string())
    }
}
