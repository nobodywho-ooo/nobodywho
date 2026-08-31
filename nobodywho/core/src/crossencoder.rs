use crate::errors::{CrossEncoderWorkerError, InitWorkerError};
use crate::inference::{BatchedReadError, EngineContext};
use crate::llm;
use crate::llm::{Worker, WorkerGuard};
use llama_cpp_2::context::params::LlamaPoolingType;
use std::sync::Arc;
use tracing::{error, warn};

#[derive(Clone)]
pub struct CrossEncoder {
    async_handle: CrossEncoderAsync,
}

#[derive(Clone)]
pub struct CrossEncoderAsync {
    guard: Arc<WorkerGuard<CrossEncoderMsg>>,
}

impl CrossEncoder {
    pub fn new(model: Arc<llm::Model>, n_ctx: u32) -> Self {
        let async_handle = CrossEncoderAsync::new(model, n_ctx);
        Self { async_handle }
    }

    pub fn rank(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<f32>, CrossEncoderWorkerError> {
        futures::executor::block_on(async { self.async_handle.rank(query, documents).await })
    }

    pub fn rank_and_sort(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<(String, f32)>, CrossEncoderWorkerError> {
        futures::executor::block_on(async {
            self.async_handle.rank_and_sort(query, documents).await
        })
    }
}

impl CrossEncoderAsync {
    pub fn new(model: Arc<llm::Model>, n_ctx: u32) -> Self {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel();

        let join_handle = std::thread::spawn(move || {
            let worker = Worker::new_crossencoder_worker(&model, n_ctx);
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

    pub async fn rank(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<f32>, CrossEncoderWorkerError> {
        let (scores_tx, mut scores_rx) = tokio::sync::mpsc::channel(1);
        self.guard.send(CrossEncoderMsg::Rank {
            query,
            documents,
            output_tx: scores_tx,
        });
        scores_rx
            .recv()
            .await
            .ok_or(CrossEncoderWorkerError::NoResponse)?
    }

    pub async fn rank_and_sort(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<(String, f32)>, CrossEncoderWorkerError> {
        let scores = self.rank(query, documents.clone()).await?;

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

enum CrossEncoderMsg {
    Rank {
        query: String,
        documents: Vec<String>,
        output_tx: tokio::sync::mpsc::Sender<Result<Vec<f32>, CrossEncoderWorkerError>>,
    },
}

/// Handle one message, reporting success or failure on its reply channel.
fn process_worker_msg(worker_state: &mut Worker<'_, CrossEncoderWorker>, msg: CrossEncoderMsg) {
    match msg {
        CrossEncoderMsg::Rank {
            query,
            documents,
            output_tx,
        } => {
            let scores = worker_state.rank(query, documents);

            let _ = output_tx.blocking_send(scores);
        }
    }
}

struct CrossEncoderWorker {}

impl llm::PoolingType for CrossEncoderWorker {
    fn pooling_type(&self) -> LlamaPoolingType {
        LlamaPoolingType::Rank
    }
}

impl<'a> Worker<'a, CrossEncoderWorker> {
    pub fn new_crossencoder_worker(
        model: &llm::Model,
        n_ctx: u32,
    ) -> Result<Worker<'_, CrossEncoderWorker>, InitWorkerError> {
        Worker::new_with_type(model, n_ctx, true, None, None, CrossEncoderWorker {})
    }

    pub fn rank(
        &mut self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<f32>, CrossEncoderWorkerError> {
        // Get CLS and SEP tokens from the model (CLS = BOS per llama.cpp, the current CLS token is deprecated.)
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let cls = self
            .engine
            .ctx
            .model
            .token_to_piece(self.engine.ctx.model.token_bos(), &mut decoder, true, None)
            .unwrap_or_else(|_| {
                warn!("Failed to convert BOS/CLS token to string, using fallback");
                "<s>".to_string()
            });

        let sep = self
            .engine
            .ctx
            .model
            .token_to_piece(self.engine.ctx.model.token_sep(), &mut decoder, true, None)
            .unwrap_or_else(|_| {
                warn!("Failed to convert SEP token to string, using fallback");
                "</s>".to_string()
            });

        let inputs = documents
            .into_iter()
            .map(|document| format!("{cls}{query}{sep}{document}{sep}"))
            .collect();

        self.engine
            .read_strings_batched(inputs, classification_score_for)
            .map_err(|error| match error {
                BatchedReadError::Read(error) => CrossEncoderWorkerError::Read(error),
                BatchedReadError::Output(error) => error,
            })
    }
}

fn classification_score_for(
    ctx: &EngineContext<'_>,
    sequence_id: i32,
) -> Result<f32, CrossEncoderWorkerError> {
    // Rank pooling returns the classification head for each sequence.
    let embeddings = ctx.embeddings_seq_ith(sequence_id)?;
    embeddings
        .first()
        .copied()
        .ok_or(CrossEncoderWorkerError::EmptyClassificationHead)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils;
    use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

    #[tokio::test]
    async fn test_crossencoder_async() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_crossencoder_model();
        let handle: CrossEncoderAsync = CrossEncoderAsync::new(model, 4096);

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
        test_utils::init_test_tracing();
        let model = test_utils::load_crossencoder_model();
        let encoder = CrossEncoder::new(model, 48);
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
            assert!(
                (individual_score - batched_score).abs() < 0.05,
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
        let encoder = CrossEncoder::new(model, 4096);
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
        test_utils::init_test_tracing();
        let model = test_utils::load_crossencoder_model();
        let encoder = CrossEncoder::new(model, 4096);

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
        test_utils::init_test_tracing();
        let model = test_utils::load_crossencoder_model();
        let crossencoder = CrossEncoder::new(model, 64);

        let err = crossencoder
            .rank("query".to_string(), vec!["word ".repeat(500)])
            .unwrap_err();
        assert!(
            matches!(
                err,
                CrossEncoderWorkerError::Read(crate::errors::ReadError::InputExceedsContext { .. })
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
