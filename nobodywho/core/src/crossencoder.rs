use crate::errors::{CrossEncoderError, InitWorkerError};
use crate::inference::{BatchedReadError, EngineContext, InferenceEngine};
use crate::llm;
use crate::worker::AsyncWorker;
use llama_cpp_2::context::params::LlamaPoolingType;
use std::sync::Arc;
use tracing::warn;
use yoke::Yoke;

#[derive(Debug)]
pub struct CrossEncoder {
    engine: Yoke<InferenceEngine<'static>, Arc<llm::Model>>,
}

impl CrossEncoder {
    pub fn new(model: Arc<llm::Model>, n_ctx: u32) -> Result<Self, InitWorkerError> {
        let engine = Yoke::try_attach_to_cart(model, |model| {
            InferenceEngine::new(model, n_ctx, true, None, None, LlamaPoolingType::Rank)
        })?;
        Ok(Self { engine })
    }

    pub fn rank(
        &mut self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<f32>, CrossEncoderError> {
        let vocab = self.engine.backing_cart().language_model.vocab();

        let bos = vocab.bos();
        let bos = if bos.0 != -1 {
            let bos = vocab.token_to_piece(bos, true, None);
            let (bos, _) = encoding_rs::UTF_8.decode_without_bom_handling(&bos);
            bos.to_string()
        } else {
            "".to_string()
        };

        let sep = vocab.sep();
        let sep = if sep.0 != -1 {
            let sep = vocab.token_to_piece(sep, true, None);
            let (sep, _) = encoding_rs::UTF_8.decode_without_bom_handling(&sep);
            sep.to_string()
        } else {
            "".to_string()
        };

        let inputs = documents
            .into_iter()
            .map(|document| format!("{bos}{query}{sep}{document}{sep}"))
            .collect();

        self.engine
            .with_mut_return(|e| e.read_strings_batched(inputs, classification_score_for))
            .map_err(|error| match error {
                BatchedReadError::Read(error) => CrossEncoderError::Read(error),
                BatchedReadError::Output(error) => error,
            })
    }

    pub fn rank_and_sort(
        &mut self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<(String, f32)>, CrossEncoderError> {
        let scores = self.rank(query, documents.clone())?;

        let mut docs_with_scores: Vec<(String, f32)> = documents
            .iter()
            .zip(scores.iter())
            .map(|(doc, score)| (doc.clone(), *score))
            .collect();

        docs_with_scores.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or_else(|| {
                warn!("Got NaN while sorting cross-encoded documents.");
                std::cmp::Ordering::Equal
            })
        });
        Ok(docs_with_scores)
    }
}

fn classification_score_for(
    ctx: &EngineContext<'_>,
    sequence_id: i32,
) -> Result<f32, CrossEncoderError> {
    // Rank pooling returns the classification head for each sequence.
    let embeddings = ctx.embeddings_seq_ith(sequence_id)?;
    embeddings
        .first()
        .copied()
        .ok_or(CrossEncoderError::EmptyClassificationHead)
}

#[derive(Debug, Clone)]
pub struct CrossEncoderAsync {
    encoder: Arc<tokio::sync::Mutex<CrossEncoder>>,
    worker: Arc<AsyncWorker>,
}

impl CrossEncoderAsync {
    pub async fn new(model: Arc<llm::Model>, n_ctx: u32) -> Result<Self, InitWorkerError> {
        let worker = AsyncWorker::new();
        let encoder = worker.run(move || CrossEncoder::new(model, n_ctx)).await?;
        Ok(Self {
            encoder: Arc::new(tokio::sync::Mutex::new(encoder)),
            worker: Arc::new(worker),
        })
    }

    pub async fn rank(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<f32>, CrossEncoderError> {
        let mut encoder = Arc::clone(&self.encoder).lock_owned().await;
        self.worker
            .run(move || encoder.rank(query, documents))
            .await
    }

    pub async fn rank_and_sort(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<(String, f32)>, CrossEncoderError> {
        let mut encoder = Arc::clone(&self.encoder).lock_owned().await;
        self.worker
            .run(move || encoder.rank_and_sort(query, documents))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils;
    use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

    #[tokio::test]
    async fn test_crossencoder_async() -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_crossencoder_model();
        let handle = CrossEncoderAsync::new(model, 4096).await?;

        let query = "What is the capital of France?".to_string();
        let mut documents = vec![
            "The Eiffel Tower is a famous landmark in the capital of France.".to_string(),
            "France is a country in Europe.".to_string(),
            "Lyon is a major city in France, but not the capital.".to_string(),
            "The capital of Germany is France.".to_string(),
            "The French government is based in Paris.".to_string(),
            "France's capital city is known for its art and culture, it is called Paris.".to_string(),
            "The Louvre Museum is located in Paris, France - which is the largest city, and the seat of the government".to_string(),
            "Paris is the capital of France.".to_string(),
            "Paris is not the capital of France.".to_string(),
            "The president of France works in Paris, the main city of his country.".to_string(),
            "What is the capital of France?".to_string(),
        ];
        let mut rng = StdRng::seed_from_u64(42);
        documents.shuffle(&mut rng);

        let ranked_docs = handle.rank_and_sort(query, documents.clone()).await?;
        let best_docs: Vec<String> = ranked_docs
            .iter()
            .take(4)
            .map(|(doc, _)| doc.to_owned())
            .collect();

        let seen_paris = best_docs.contains(&"Paris is the capital of France.".to_string());

        assert!(
            seen_paris,
            "`Paris is the capital of France.` was not between the best four, the best three were: {}",
            best_docs.join(",")
        );

        Ok(())
    }

    #[test]
    fn test_crossencoder_batch_matches_individual_scores() -> Result<(), Box<dyn std::error::Error>>
    {
        let model = test_utils::load_crossencoder_model();
        let mut encoder = CrossEncoder::new(model, 48)?;
        let query = "What is the capital of France?".to_string();
        let documents = vec![
            "Paris is the capital of France.".to_string(),
            "Berlin is the capital of Germany.".to_string(),
            "The weather is nice today.".to_string(),
            "The French government is based in Paris.".to_string(),
        ];

        let individual_scores = documents
            .iter()
            .map(|document| {
                encoder
                    .rank(query.clone(), vec![document.clone()])
                    .map(|scores| scores[0])
            })
            .collect::<Result<Vec<_>, _>>()?;
        let batched_scores = encoder.rank(query, documents)?;

        assert_eq!(individual_scores.len(), batched_scores.len());
        for (individual_score, batched_score) in individual_scores.iter().zip(&batched_scores) {
            // Scores should be close but might not be identical due to batching and GPU offloading.
            // Batch size changes the ggml kernel, and llama.cpp offloads ops to the GPU at >=32
            // rows, so scores drift. The ordering assertion below is the real check.
            assert!(
                (individual_score - batched_score).abs() < 0.25,
                "Individual score {individual_score} differed from batched score {batched_score}"
            );
        }

        let mut individual_order = (0..individual_scores.len()).collect::<Vec<_>>();
        individual_order.sort_by(|a, b| individual_scores[*b].total_cmp(&individual_scores[*a]));
        let mut batched_order = (0..batched_scores.len()).collect::<Vec<_>>();
        batched_order.sort_by(|a, b| batched_scores[*b].total_cmp(&batched_scores[*a]));
        assert_eq!(individual_order, batched_order);

        Ok(())
    }

    #[test]
    #[ignore = "real-model benchmark; run with --ignored --nocapture"]
    fn benchmark_crossencoder_batching() -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_crossencoder_model();
        let mut encoder = CrossEncoder::new(model, 4096)?;
        let query = "What is the capital of France?".to_string();
        let documents = (0..40)
            .map(|index| {
                format!(
                    "Document {index}: Paris is the capital of France and the seat of its government."
                )
            })
            .collect::<Vec<_>>();

        let sequential_start = std::time::Instant::now();
        let sequential_scores = documents
            .iter()
            .map(|document| {
                encoder
                    .rank(query.clone(), vec![document.clone()])
                    .map(|scores| scores[0])
            })
            .collect::<Result<Vec<_>, _>>()?;
        let sequential_elapsed = sequential_start.elapsed();

        let batched_start = std::time::Instant::now();
        let batched_scores = encoder.rank(query, documents.clone())?;
        let batched_elapsed = batched_start.elapsed();

        let max_score_difference = sequential_scores
            .iter()
            .zip(&batched_scores)
            .map(|(sequential_score, batched_score)| (sequential_score - batched_score).abs())
            .fold(0.0, f32::max);

        eprintln!(
            "Sequential: {} one-document rank calls in {:?}",
            documents.len(),
            sequential_elapsed
        );
        eprintln!(
            "Batched: one {}-document rank call in {:?} ({:.2}x faster, max score difference {:.6})",
            documents.len(),
            batched_elapsed,
            sequential_elapsed.as_secs_f64() / batched_elapsed.as_secs_f64(),
            max_score_difference
        );

        Ok(())
    }

    #[test]
    fn test_crossencoder_sync() -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_crossencoder_model();
        let mut encoder = CrossEncoder::new(model, 4096)?;

        let query = "What is the capital of France?".to_string();
        let documents = vec![
            "The Eiffel Tower is a famous landmark in the capital of France.".to_string(),
            "France is a country in Europe.".to_string(),
            "Lyon is a major city in France, but not the capital.".to_string(),
            "The capital of Germany is France.".to_string(),
            "The French government is based in Paris.".to_string(),
            "France's capital city is known for its art and culture, it is called Paris.".to_string(),
            "The Louvre Museum is located in Paris, France - which is the largest city, and the seat of the government".to_string(),
            "Paris is the capital of France.".to_string(),
            "Paris is not the capital of France.".to_string(),
            "The president of France works in Paris, the main city of his country.".to_string(),
            "What is the capital of France?".to_string(),
        ];

        let ranked_docs = encoder.rank_and_sort(query, documents.clone())?;
        let best_docs: Vec<String> = ranked_docs
            .iter()
            .take(4)
            .map(|(doc, _)| doc.to_owned())
            .collect();

        let seen_paris = best_docs.contains(&"Paris is the capital of France.".to_string());

        assert!(
            seen_paris,
            "`Paris is the capital of France.` was not between the best four, the best three were: {}",
            best_docs.join(",")
        );

        Ok(())
    }

    #[test]
    fn test_oversized_input_keeps_crossencoder_alive() {
        let model = test_utils::load_crossencoder_model();
        let mut crossencoder = CrossEncoder::new(model, 64).unwrap();

        let err = crossencoder
            .rank("query".to_string(), vec!["word ".repeat(500)])
            .unwrap_err();
        assert!(
            matches!(
                err,
                CrossEncoderError::Read(crate::errors::ReadError::InputExceedsContext { .. })
            ),
            "the read error should reach the caller, got {err:?}"
        );

        crossencoder
            .rank(
                "What is the capital of France?".to_string(),
                vec!["Paris is the capital of France.".to_string()],
            )
            .expect("worker still answers");
    }
}
