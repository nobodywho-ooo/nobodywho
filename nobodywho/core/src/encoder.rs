use crate::errors::{EncoderWorkerError, InitWorkerError};
use crate::inference::{BatchedReadError, InferenceEngine};
use crate::llm;
use crate::llm::WorkerGuard;
use llama_cpp_2::context::params::LlamaPoolingType;
use std::sync::Arc;
use tracing::error;

#[derive(Clone)]
pub struct Encoder {
    async_handle: EncoderAsync,
}

#[derive(Clone)]
pub struct EncoderAsync {
    guard: Arc<WorkerGuard<EncoderMsg>>,
}

impl Encoder {
    pub fn new(model: Arc<llm::Model>, n_ctx: u32) -> Self {
        let async_handle = EncoderAsync::new(model, n_ctx);
        Self { async_handle }
    }

    pub fn encode(&self, text: String) -> Result<Vec<f32>, EncoderWorkerError> {
        futures::executor::block_on(async { self.async_handle.encode(text).await })
    }

    pub fn encode_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, EncoderWorkerError> {
        futures::executor::block_on(async { self.async_handle.encode_batch(texts).await })
    }
}

impl EncoderAsync {
    pub fn new(model: Arc<llm::Model>, n_ctx: u32) -> Self {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel();

        let join_handle = std::thread::spawn(move || {
            let worker = EncoderWorker::new(&model, n_ctx);
            let mut worker_state = match worker {
                Ok(worker_state) => worker_state,
                Err(errmsg) => {
                    return error!(error=%errmsg, "Could not set up the worker initial state")
                }
            };

            while let Ok(msg) = msg_rx.recv() {
                process_worker_msg(&mut worker_state, msg);
            }
        });

        Self {
            guard: Arc::new(WorkerGuard::new(msg_tx, join_handle, None)),
        }
    }

    pub async fn encode(&self, text: String) -> Result<Vec<f32>, EncoderWorkerError> {
        self.encode_batch(vec![text])
            .await?
            .pop()
            .ok_or_else(|| EncoderWorkerError::Encode("Encoder returned no embedding.".into()))
    }

    pub async fn encode_batch(
        &self,
        texts: Vec<String>,
    ) -> Result<Vec<Vec<f32>>, EncoderWorkerError> {
        let (embedding_tx, mut embedding_rx) = tokio::sync::mpsc::channel(1);
        self.guard.send(EncoderMsg::EncodeBatch {
            texts,
            output_tx: embedding_tx,
        });
        embedding_rx.recv().await.ok_or(EncoderWorkerError::Encode(
            "Could not encode the texts. Worker never responded.".into(),
        ))?
    }
}

enum EncoderMsg {
    EncodeBatch {
        texts: Vec<String>,
        output_tx: tokio::sync::mpsc::Sender<Result<Vec<Vec<f32>>, EncoderWorkerError>>,
    },
}

/// Handle one message, reporting success or failure on its reply channel.
fn process_worker_msg(worker_state: &mut EncoderWorker<'_>, msg: EncoderMsg) {
    match msg {
        EncoderMsg::EncodeBatch { texts, output_tx } => {
            let pooling = worker_state.pooling;
            let embeddings = worker_state
                .engine
                .read_strings_batched::<_, EncoderWorkerError>(texts, |ctx, sequence_id| {
                    let embedding = if pooling == LlamaPoolingType::None {
                        ctx.embeddings_ith(-1)?
                    } else {
                        ctx.embeddings_seq_ith(sequence_id)?
                    };
                    Ok(embedding.to_vec())
                })
                .map_err(|error| match error {
                    BatchedReadError::Read(error) => EncoderWorkerError::Read(error),
                    BatchedReadError::Output(error) => error,
                });
            let _ = output_tx.blocking_send(embeddings);
        }
    }
}

struct EncoderWorker<'a> {
    engine: InferenceEngine<'a>,
    pooling: LlamaPoolingType,
}

impl<'a> EncoderWorker<'a> {
    fn new(model: &'a llm::Model, n_ctx: u32) -> Result<Self, InitWorkerError> {
        let arch = model
            .language_model
            .meta_val_str("general.architecture")
            .unwrap_or_default();
        let key = format!("{arch}.pooling_type");
        let pooling = model
            .language_model
            .meta_val_str(&key)
            .ok()
            .and_then(|val| val.parse::<i32>().ok())
            .map(LlamaPoolingType::from)
            .unwrap_or(LlamaPoolingType::Unspecified);
        let engine = InferenceEngine::new_with_type(model, n_ctx, true, None, None, pooling)?;
        Ok(Self { engine, pooling })
    }

    #[cfg(test)]
    fn get_embedding(&self) -> Result<Vec<f32>, llama_cpp_2::EmbeddingsSeqError> {
        Ok(self.engine.ctx.embeddings_seq_ith(0)?.to_vec())
    }

    /// Tokenize `text` and read it into the context under the global inference lock.
    #[cfg(test)]
    #[tracing::instrument(level = "trace", skip(self))]
    fn read_string(&mut self, text: String) -> Result<&mut Self, crate::errors::ReadError> {
        let chunks = self.engine.tokenize(text, vec![])?;
        self.engine.read_chunks(chunks)?;
        Ok(self)
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
        let model = test_utils::load_embeddings_model();

        let mut worker = EncoderWorker::new(&model, 1024)?;

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
    fn test_encoder_batch_matches_individual_embeddings() -> Result<(), Box<dyn std::error::Error>>
    {
        let model = test_utils::load_embeddings_model();
        let encoder = Encoder::new(model, 20);
        let texts = vec![
            "Copenhagen is the capital of Denmark.".to_string(),
            "Berlin is the capital of Germany.".to_string(),
            "The weather is nice today.".to_string(),
        ];

        let individual = texts
            .iter()
            .map(|text| encoder.encode(text.clone()))
            .collect::<Result<Vec<_>, _>>()?;
        let batched = encoder.encode_batch(texts)?;

        assert_eq!(individual.len(), batched.len());
        for (individual_embedding, batched_embedding) in individual.iter().zip(&batched) {
            assert_eq!(individual_embedding.len(), batched_embedding.len());
            assert!(individual_embedding.iter().zip(batched_embedding).all(
                |(individual_value, batched_value)| (individual_value - batched_value).abs() < 1e-5
            ));
        }

        Ok(())
    }

    #[test]
    fn test_deterministic_encoder() -> Result<(), Box<dyn std::error::Error>> {
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

    #[test]
    fn test_oversized_input_keeps_encoder_alive() {
        let model = test_utils::load_embeddings_model();
        let encoder = Encoder::new(model, 64);

        let err = encoder.encode("word ".repeat(500)).unwrap_err();
        assert!(
            matches!(
                err,
                EncoderWorkerError::Read(crate::errors::ReadError::InputExceedsContext { .. })
            ),
            "the read error should reach the caller, got {err:?}"
        );

        encoder
            .encode("Copenhagen is the capital of Denmark.".to_string())
            .expect("worker still answers");
    }
}
