pub mod chat;
pub mod crossencoder;
pub mod encoder;
pub mod errors;
pub mod llm;
pub mod sampler_config;
pub mod template;

pub fn send_llamacpp_logs_to_tracing() {
    llama_cpp_2::send_logs_to_tracing(llama_cpp_2::LogOptions::default().with_logs_enabled(true));
}

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

    /// Get path to test crossencoder model from TEST_CROSSENCODER_MODEL env var
    pub fn test_crossencoder_model_path() -> String {
        std::env::var("TEST_CROSSENCODER_MODEL").unwrap_or_else(|_| "crossencoder.gguf".to_string())
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
        // XXX: loading the embeddings model for unit tests without GPU offloading
        //      because it otherwise caused a segfault specifically with the llvmpipe vulkan driver.
        //      (which is used in the nix sandbox, since we don't have access to the host GPU)
        //      llvmpipe is very rare in the wild, so it shouldn't cause any problems in general
        //      this segfault doesn't happen on nobodywho commit 94d51c5.
        //      it's most likely related to an upstream change in llama.cpp
        get_model(&path, false)
            .unwrap_or_else(|e| panic!("Failed to load embeddings model from {}: {:?}", path, e))
    }

    /// Load the crossencoder model with GPU acceleration if available
    pub fn load_crossencoder_model() -> Model {
        let path = test_crossencoder_model_path();
        // Same GPU offloading note as embeddings model
        get_model(&path, false)
            .unwrap_or_else(|e| panic!("Failed to load crossencoder model from {}: {:?}", path, e))
    }
}
