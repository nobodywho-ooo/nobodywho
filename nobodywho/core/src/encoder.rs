use crate::errors::{EncoderError, InitWorkerError};
use crate::inference::{BatchedReadError, InferenceEngine};
use crate::llm;
use crate::worker::AsyncWorker;
use llama_cpp_2::context::params::LlamaPoolingType;
use std::sync::Arc;
use yoke::Yoke;

#[derive(Debug)]
pub struct Encoder {
    engine: sync_wrapper::SyncWrapper<Yoke<InferenceEngine<'static>, Arc<llm::Model>>>,
    pooling: LlamaPoolingType,
}

impl Encoder {
    pub fn new(model: Arc<llm::Model>, n_ctx: u32) -> Result<Self, InitWorkerError> {
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
        let engine = Yoke::try_attach_to_cart(model, |model| {
            InferenceEngine::new(model, n_ctx, true, None, None, pooling)
        })?;
        let engine = sync_wrapper::SyncWrapper::new(engine);
        Ok(Self { engine, pooling })
    }

    pub fn encode_batch(&mut self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, EncoderError> {
        let pooling = self.pooling;
        self.engine
            .get_mut()
            .with_mut_return(move |e| {
                e.read_strings_batched::<_, EncoderError>(texts, |ctx, sequence_id| {
                    let embedding = if pooling == LlamaPoolingType::None {
                        ctx.embeddings_ith(-1)?
                    } else {
                        ctx.embeddings_seq_ith(sequence_id)?
                    };
                    Ok(embedding.to_vec())
                })
            })
            .map_err(|error| match error {
                BatchedReadError::Read(error) => EncoderError::Read(error),
                BatchedReadError::Output(error) => error,
            })
    }

    pub fn encode(&mut self, text: String) -> Result<Vec<f32>, EncoderError> {
        self.encode_batch(vec![text])?
            .pop()
            .ok_or(EncoderError::NoEmbedding)
    }

    #[cfg(test)]
    fn get_embedding(&mut self) -> Result<Vec<f32>, llama_cpp_2::EmbeddingsSeqError> {
        Ok(self
            .engine
            .get_mut()
            .get()
            .ctx
            .embeddings_seq_ith(0)?
            .to_vec())
    }

    /// Tokenize `text` and read it into the context.
    #[cfg(test)]
    #[tracing::instrument(level = "trace", skip(self))]
    fn read_string(&mut self, text: String) -> Result<&mut Self, crate::errors::ReadError> {
        let chunks = self.engine.get_mut().get().tokenize(text, vec![])?;
        self.engine
            .get_mut()
            .with_mut_return(|e| e.read_chunks(chunks))?;
        Ok(self)
    }
}

#[derive(Debug, Clone)]
pub struct EncoderAsync {
    encoder: Arc<tokio::sync::Mutex<Encoder>>,
    worker: Arc<AsyncWorker>,
}

impl EncoderAsync {
    pub async fn new(model: Arc<llm::Model>, n_ctx: u32) -> Result<Self, InitWorkerError> {
        let worker = AsyncWorker::new();
        let encoder = worker.run(move || Encoder::new(model, n_ctx)).await?;
        Ok(Self {
            encoder: Arc::new(tokio::sync::Mutex::new(encoder)),
            worker: Arc::new(worker),
        })
    }

    pub async fn encode(&self, text: String) -> Result<Vec<f32>, EncoderError> {
        let mut encoder = Arc::clone(&self.encoder).lock_owned().await;
        self.worker.run(move || encoder.encode(text)).await
    }

    pub async fn encode_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, EncoderError> {
        let mut encoder = Arc::clone(&self.encoder).lock_owned().await;
        self.worker.run(move || encoder.encode_batch(texts)).await
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
        let mut encoder = Encoder::new(model, 1024)?;

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

        let mut encoder = Encoder::new(model, 1024)?;

        let copenhagen_embedding = encoder
            .read_string("Copenhagen is the capital of Denmark.".to_string())?
            .get_embedding()?;

        let berlin_embedding = encoder
            .read_string("Berlin is the capital of Germany.".to_string())?
            .get_embedding()?;

        let insult_embedding = encoder
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
        let mut encoder = Encoder::new(model, 20)?;
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
        let mut encoder = Encoder::new(model, 1024)?;

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
        let mut encoder = Encoder::new(model, 64).unwrap();

        let err = encoder.encode("word ".repeat(500)).unwrap_err();
        assert!(
            matches!(
                err,
                EncoderError::Read(crate::errors::ReadError::InputExceedsContext { .. })
            ),
            "the read error should reach the caller, got {err:?}"
        );

        encoder
            .encode("Copenhagen is the capital of Denmark.".to_string())
            .expect("worker still answers");
    }
}
