// pub mod chat;
pub mod chat_state;
pub mod llm;
// pub mod llmactor;
pub mod chatworker;
pub mod sampler_config;

#[cfg(test)]
pub mod test_utils {
    use crate::llm::{get_model, Model};
    use std::sync::Once;

    static INIT: Once = Once::new();

    /// Initialize tracing for tests
    pub fn init_test_tracing() {
        INIT.call_once(|| {
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::TRACE)
                .with_timer(tracing_subscriber::fmt::time::uptime())
                .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
                .try_init()
                .ok();
        });
    }

    /// Get path to test model from TEST_MODEL env var or default to "model.gguf"
    pub fn test_model_path() -> String {
        std::env::var("TEST_MODEL").unwrap_or_else(|_| "model.gguf".to_string())
    }

    /// Get path to test embeddings model from TEST_EMBEDDINGS_MODEL env var
    pub fn test_embeddings_model_path() -> String {
        std::env::var("TEST_EMBEDDINGS_MODEL").unwrap_or_else(|_| "embeddings.gguf".to_string())
    }

    /// Load the test model with GPU acceleration if available
    pub fn load_test_model() -> Model {
        let path = test_model_path();
        get_model(&path, true)
            .unwrap_or_else(|e| panic!("Failed to load test model from {}: {:?}", path, e))
    }

    /// Load the embeddings model with GPU acceleration if available
    pub fn load_embeddings_model() -> Model {
        let path = test_embeddings_model_path();
        get_model(&path, true)
            .unwrap_or_else(|e| panic!("Failed to load embeddings model from {}: {:?}", path, e))
    }
}
