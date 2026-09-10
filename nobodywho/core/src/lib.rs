pub mod chat;
pub mod content;
pub mod cpu;
pub mod crossencoder;
pub mod encoder;
pub mod errors;
mod host_memory;
pub mod huggingface;
pub mod inference;
pub mod llm;
pub mod memory;
mod model_selection;
pub mod onnx;
pub mod sampler;
pub mod speech_to_text;
pub mod stream;
pub mod template;
pub mod text_to_speech;
pub mod tokenizer;
pub mod tool_calling;
pub mod voice_activity_detection;

/// Re-exported so bindings can name `Diagnostic` without depending on miette directly.
pub use miette;

/// Render a miette diagnostic to a plain-text string, including any `help` text,
/// error codes, and related errors. Falls back to `to_string()` if rendering fails.
///
/// Uses `NarratableReportHandler` (no ANSI colour codes) so the output is safe
/// to embed in Python exceptions, Dart/Flutter error objects, GDScript strings,
/// and React Native JS errors.
pub fn render_miette(err: &dyn miette::Diagnostic) -> String {
    let mut out = String::new();
    if miette::NarratableReportHandler::new()
        .render_report(&mut out, err)
        .is_err()
    {
        out = err.to_string();
    }
    out
}

pub fn send_llamacpp_logs_to_tracing() {
    llama_cpp_2::send_logs_to_tracing(llama_cpp_2::LogOptions::default().with_logs_enabled(true));
}

#[cfg(test)]
pub(crate) mod test_utils {
    use crate::llm::{get_model, Model};
    use crate::send_llamacpp_logs_to_tracing;
    use std::sync::{Arc, Once};

    static INIT: Once = Once::new();

    /// Initialize tracing for tests
    pub(crate) fn init_test_tracing() {
        INIT.call_once(|| {
            send_llamacpp_logs_to_tracing();

            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::INFO)
                .with_timer(tracing_subscriber::fmt::time::uptime())
                .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
                .with_test_writer()
                .init();
        });
    }

    /// Load the test model with GPU acceleration if available
    pub(crate) fn load_test_model() -> Arc<Model> {
        let path = std::env::var("TEST_MODEL").unwrap_or_else(|_| "model.gguf".to_string());
        Arc::new(
            get_model(&path, true, None, None, None)
                .unwrap_or_else(|e| panic!("failed to load test model from {path}: {e}")),
        )
    }

    /// Load the embeddings model with GPU acceleration if available
    pub(crate) fn load_embeddings_model() -> Arc<Model> {
        let path = std::env::var("TEST_EMBEDDINGS_MODEL")
            .unwrap_or_else(|_| "embeddings.gguf".to_string());
        // XXX: loading the embeddings model for unit tests without GPU offloading
        //      because it otherwise caused a segfault specifically with the llvmpipe vulkan driver.
        //      (which is used in the nix sandbox, since we don't have access to the host GPU)
        //      llvmpipe is very rare in the wild, so it shouldn't cause any problems in general
        //      this segfault doesn't happen on nobodywho commit 94d51c5.
        //      it's most likely related to an upstream change in llama.cpp
        Arc::new(
            get_model(&path, false, None, None, None)
                .unwrap_or_else(|e| panic!("failed to load embeddings model from {path}: {e}")),
        )
    }

    /// Load the crossencoder model with GPU acceleration if available
    pub(crate) fn load_crossencoder_model() -> Arc<Model> {
        let path = std::env::var("TEST_CROSSENCODER_MODEL")
            .unwrap_or_else(|_| "crossencoder.gguf".to_string());
        // Same GPU offloading note as embeddings model
        Arc::new(
            get_model(&path, false, None, None, None)
                .unwrap_or_else(|e| panic!("failed to load crossencoder model from {path}: {e}")),
        )
    }

    /// Load the MTP draft and target models.
    pub(crate) fn load_mtp_models() -> Option<Arc<Model>> {
        let target_path = std::env::var("TEST_MTP_TARGET_MODEL").ok()?;
        let draft_path = std::env::var("TEST_MTP_DRAFT_MODEL")
            .expect("should have TEST_MTP_DRAFT_MODEL if TEST_MTP_TARGET_MODEL is set");

        Some(Arc::new(
            get_model(&target_path, true, None, Some(&draft_path), None).unwrap_or_else(|e| {
                panic!("failed to load MTP models from {target_path} and {draft_path}: {e}")
            }),
        ))
    }

    pub(crate) fn load_mtmd_models() -> Option<Arc<Model>> {
        let vision_path = std::env::var("TEST_VISION_MODEL").ok()?;
        let mmproj_path = std::env::var("TEST_MMPROJ_MODEL")
            .expect("should have TEST_MMPROJ_MODEL if TEST_VISION_MODEL is set");

        Some(Arc::new(
            get_model(&vision_path, true, Some(&mmproj_path), None, None).unwrap_or_else(|e| {
                panic!("failed to load vision models from {vision_path} and {mmproj_path}: {e}")
            }),
        ))
    }

    pub(crate) fn gemma4_model() -> Option<Arc<Model>> {
        let path = std::env::var("GEMMA4_MODEL").ok()?;
        Some(Arc::new(
            get_model(&path, true, None, None, None)
                .unwrap_or_else(|e| panic!("failed to load Gemma4 model from {path}: {e}")),
        ))
    }

    pub(crate) fn qwen36_model() -> Option<Arc<Model>> {
        let path = std::env::var("QWEN36_MODEL").ok()?;
        Some(Arc::new(
            get_model(&path, false, None, None, None)
                .unwrap_or_else(|e| panic!("failed to load Qwen3.6 model from {path}: {e}")),
        ))
    }
}
