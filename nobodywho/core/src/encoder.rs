use crate::errors::{EncoderWorkerError, InitWorkerError};
use crate::llm;
use crate::llm::Worker;
use llama_cpp_2::context::params::LlamaPoolingType;
use llama_cpp_2::model::LlamaModel;
use tracing::error;

use std::sync::Arc;

pub struct Encoder {
    async_handle: EncoderAsync,
}

pub struct EncoderAsync {
    msg_tx: std::sync::mpsc::Sender<EncoderMsg>,
}

impl Encoder {
    pub fn new(model: Arc<LlamaModel>, n_ctx: u32) -> Self {
        let async_handle = EncoderAsync::new(model, n_ctx);
        Self { async_handle }
    }

    pub fn encode(&self, text: String) -> Result<Vec<f32>, EncoderWorkerError> {
        let mut receiver = self.async_handle.encode(text);
        futures::executor::block_on(async {
            receiver
                .recv()
                .await
                .ok_or_else(|| EncoderWorkerError::Encode("Could not encode the text".to_string()))
        })
    }
}

impl EncoderAsync {
    pub fn new(model: Arc<LlamaModel>, n_ctx: u32) -> Self {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            let Ok(mut worker_state) = Worker::new_encoder_worker(&model, n_ctx) else {
                return error!("Could not set up the worker initial state");
            };

            while let Ok(msg) = msg_rx.recv() {
                if let Err(e) = process_worker_msg(&mut worker_state, msg) {
                    return error!("Encoder Worker crashed: {e}");
                }
            }
        });

        Self { msg_tx }
    }

    pub fn encode(&self, text: String) -> tokio::sync::mpsc::Receiver<Vec<f32>> {
        let (embedding_tx, embedding_rx) = tokio::sync::mpsc::channel(1);
        let _ = self.msg_tx.send(EncoderMsg::Encode(text, embedding_tx));
        embedding_rx
    }
}

enum EncoderMsg {
    Encode(String, tokio::sync::mpsc::Sender<Vec<f32>>),
}

fn process_worker_msg(
    worker_state: &mut Worker<'_, EncoderWorker>,
    msg: EncoderMsg,
) -> Result<(), EncoderWorkerError> {
    match msg {
        EncoderMsg::Encode(text, respond) => {
            worker_state.reset_context();

            let embedding = worker_state.read_string(text)?.get_embedding()?;
            let _ = respond.blocking_send(embedding);
        }
    }

    Ok(())
}

struct EncoderWorker {}

impl llm::PoolingType for EncoderWorker {
    fn pooling_type(&self) -> LlamaPoolingType {
        LlamaPoolingType::Cls
    }
}

impl<'a> Worker<'a, EncoderWorker> {
    pub fn new_encoder_worker(
        model: &Arc<LlamaModel>,
        n_ctx: u32,
    ) -> Result<Worker<'_, EncoderWorker>, InitWorkerError> {
        Worker::new_with_type(model, n_ctx, true, EncoderWorker {})
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
    fn test_encoder_sync() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_embeddings_model();
        let encoder = Encoder::new(model, 1024);

        let copenhagen_embedding =
            encoder.encode("Copenhagen is the capital of Denmark.".to_string())?;
        let berlin_embedding = encoder.encode("Berlin is the capital of Germany.".to_string())?;
        let insult_embedding = encoder.encode(
            "Your mother was a hamster and your father smelt of elderberries!".to_string(),
        )?;

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
    fn test_encoder_worker_direct() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_embeddings_model();

        let mut worker = Worker::new_encoder_worker(&model, 1024)?;

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

        assert_eq!(
            cosine_similarity(&copenhagen_embedding, &berlin_embedding),
            cosine_similarity(&berlin_embedding, &copenhagen_embedding)
        );

        assert!(
            (cosine_similarity(&copenhagen_embedding, &copenhagen_embedding) - 1.0).abs() < 0.001,
        );

        assert!(
            cosine_similarity(&copenhagen_embedding, &insult_embedding)
                < cosine_similarity(&copenhagen_embedding, &berlin_embedding)
        );

        Ok(())
    }

    #[test]
    fn test_deterministic_encoder() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_embeddings_model();
        let encoder = Encoder::new(model, 1024);

        let input = "I don't want to be different";

        let first_embedding = encoder.encode(input.to_string())?;
        let second_embedding = encoder.encode(input.to_string())?;

        assert_eq!(
            first_embedding, second_embedding,
            "Same input '{}' should produce identical embeddings.",
            input
        );

        Ok(())
    }
}
