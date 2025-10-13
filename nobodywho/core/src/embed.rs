use crate::llm;
use crate::llm::Worker;
use llama_cpp_2::context::params::LlamaPoolingType;
use llama_cpp_2::model::LlamaModel;
use tracing::error;

use std::sync::Arc;

// EmbeddingsHandle - for parallelism

pub struct EmbeddingsHandle {
    msg_tx: std::sync::mpsc::Sender<EmbeddingsMsg>,
}

impl EmbeddingsHandle {
    pub fn new(model: Arc<LlamaModel>, n_ctx: u32) -> Self {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            if let Err(e) = run_worker(model, n_ctx, msg_rx) {
                error!("Worker crashed: {}", e)
            }
        });

        Self { msg_tx }
    }

    pub fn embed_text(&self, text: String) -> tokio::sync::mpsc::Receiver<Vec<f32>> {
        let (embedding_tx, embedding_rx) = tokio::sync::mpsc::channel(1);
        let _ = self.msg_tx.send(EmbeddingsMsg::Embed(text, embedding_tx));
        embedding_rx
    }
}

enum EmbeddingsMsg {
    Embed(String, tokio::sync::mpsc::Sender<Vec<f32>>),
}

#[derive(Debug, thiserror::Error)]
enum EmbeddingsWorkerError {
    #[error("Error initializing worker: {0}")]
    InitWorkerError(#[from] llm::InitWorkerError),

    #[error("Error reading string: {0}")]
    ReadError(#[from] llm::ReadError),

    #[error("Error generating text: {0}")]
    EmbeddingsError(#[from] llama_cpp_2::EmbeddingsError),
}

fn run_worker(
    model: Arc<LlamaModel>,
    n_ctx: u32,
    msg_rx: std::sync::mpsc::Receiver<EmbeddingsMsg>,
) -> Result<(), EmbeddingsWorkerError> {
    let mut worker_state = Worker::new_embeddings_worker(&model, n_ctx)?;
    while let Ok(msg) = msg_rx.recv() {
        match msg {
            EmbeddingsMsg::Embed(text, respond) => {
                // we need to clear the kv_cache to ensure deterministic output.
                worker_state.reset_context();

                let embedding = worker_state.read_string(text)?.get_embedding()?;
                let _ = respond.blocking_send(embedding);
            }
        }
    }
    Ok(())
}

// Embeddings Worker - synchronous, blocking work

struct EmbeddingsWorker {}

impl llm::PoolingType for EmbeddingsWorker {
    fn pooling_type(&self) -> LlamaPoolingType {
        LlamaPoolingType::Cls
    }
}

impl<'a> Worker<'a, EmbeddingsWorker> {
    pub fn new_embeddings_worker(
        model: &Arc<LlamaModel>,
        n_ctx: u32,
    ) -> Result<Worker<'_, EmbeddingsWorker>, llm::InitWorkerError> {
        Worker::new_with_type(model, n_ctx, true, EmbeddingsWorker {})
    }

    pub fn get_embedding(&self) -> Result<Vec<f32>, llama_cpp_2::EmbeddingsError> {
        Ok(self.ctx.embeddings_seq_ith(0)?.to_vec())
    }
}

fn dotproduct(a: &[f32], b: &[f32]) -> f32 {
    assert!(a.len() == b.len());
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let norm_a = dotproduct(a, a).sqrt();
    let norm_b = dotproduct(b, b).sqrt();
    if norm_a == 0. || norm_b == 0. {
        return f32::NAN;
    }
    dotproduct(a, b) / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils;
    #[test]
    fn test_embeddings() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_embeddings_model();

        let mut worker = Worker::new_embeddings_worker(&model, 1024)?;

        let copenhagen_embedding = worker
            .read_string("Copenhagen is the capital of Denmark.".to_string())?
            .get_embedding()?;

        let berlin_embedding = worker
            .read_string("Berlin is the capital of Germany.".to_string())?
            .get_embedding()?;

        let insult_embedding = worker
            .read_string(
                "Your mother was a hamster and your father smelt of elderberries!".to_string(),
            )?
            .get_embedding()?;

        assert!(
            insult_embedding.len() == berlin_embedding.len()
                && berlin_embedding.len() == copenhagen_embedding.len()
                && copenhagen_embedding.len() == insult_embedding.len(),
            "not all embedding lengths were equal"
        );

        // cosine similarity should not care about order
        assert_eq!(
            cosine_similarity(&copenhagen_embedding, &berlin_embedding),
            cosine_similarity(&berlin_embedding, &copenhagen_embedding)
        );

        // any vector should have cosine similarity 1 to itself
        // (tolerate small float error)
        assert!(
            (cosine_similarity(&copenhagen_embedding, &copenhagen_embedding) - 1.0).abs() < 0.001,
        );

        // the insult should have a lower similarity than the two geography sentences
        assert!(
            cosine_similarity(&copenhagen_embedding, &insult_embedding)
                < cosine_similarity(&copenhagen_embedding, &berlin_embedding)
        );

        Ok(())
    }

    #[test]
    fn test_deterministic_embeddings() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_embeddings_model();
        let mut worker = Worker::new_embeddings_worker(&model, 1024)?;

        let input = "I don't want to be different";

        let first_embedding = worker.read_string(input.to_string())?.get_embedding()?;

        worker.reset_context();

        let second_embedding = worker.read_string(input.to_string())?.get_embedding()?;

        assert_eq!(
            first_embedding, second_embedding,
            "Same input '{}' should produce identical embeddings.",
            input
        );

        Ok(())
    }
}
