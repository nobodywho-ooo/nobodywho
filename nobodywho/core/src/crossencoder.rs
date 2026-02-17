use crate::errors::{CrossEncoderWorkerError, InitWorkerError};
use crate::llm;
use crate::llm::Worker;
use llama_cpp_2::context::params::LlamaPoolingType;
use llama_cpp_2::model::LlamaModel;
use std::sync::{Arc, Mutex};
use tracing::{error, warn};

#[derive(Clone)]
pub struct CrossEncoder {
    async_handle: CrossEncoderAsync,
}

#[derive(Clone)]
pub struct CrossEncoderAsync {
    msg_tx: Arc<Mutex<Option<std::sync::mpsc::Sender<CrossEncoderMsg>>>>,
    join_handle: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
}

impl CrossEncoder {
    pub fn new(model: Arc<LlamaModel>, n_ctx: u32) -> Self {
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
    pub fn new(model: Arc<LlamaModel>, n_ctx: u32) -> Self {
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
                if let Err(e) = process_worker_msg(&mut worker_state, msg) {
                    return error!(error=%e, "Cross-encoder worker crashed");
                }
            }
        });

        Self {
            msg_tx: Arc::new(Mutex::new(Some(msg_tx))),
            join_handle: Arc::new(Mutex::new(Some(join_handle))),
        }
    }

    pub async fn rank(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<f32>, CrossEncoderWorkerError> {
        let (scores_tx, mut scores_rx) = tokio::sync::mpsc::channel(1);
        if let Ok(guard) = self.msg_tx.lock() {
            if let Some(ref msg_tx) = *guard {
                let _ = msg_tx.send(CrossEncoderMsg::Rank(query, documents, scores_tx));
            }
        }
        scores_rx
            .recv()
            .await
            .ok_or(CrossEncoderWorkerError::NoResponse)
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

impl Drop for CrossEncoderAsync {
    fn drop(&mut self) {
        // Only join on the last reference
        if Arc::strong_count(&self.join_handle) == 1 {
            // Drop the sender to close the channel
            if let Ok(mut tx_guard) = self.msg_tx.lock() {
                drop(tx_guard.take());
            }

            // Give thread time to exit (ranking operations can't be interrupted, so we wait longer)
            std::thread::sleep(std::time::Duration::from_millis(1000));

            // Join the thread
            if let Ok(mut guard) = self.join_handle.lock() {
                if let Some(handle) = guard.take() {
                    match handle.join() {
                        Ok(()) => {}
                        Err(e) => error!("CrossEncoder worker panicked: {:?}", e),
                    }
                }
            }
        }
    }
}

enum CrossEncoderMsg {
    Rank(String, Vec<String>, tokio::sync::mpsc::Sender<Vec<f32>>),
}

fn process_worker_msg(
    worker_state: &mut Worker<'_, CrossEncoderWorker>,
    msg: CrossEncoderMsg,
) -> Result<(), CrossEncoderWorkerError> {
    match msg {
        CrossEncoderMsg::Rank(query, documents, respond) => {
            // Clear context for each cross-encoder operation
            worker_state.reset_context();

            let scores = worker_state.rank(query, documents)?;

            let _ = respond.blocking_send(scores);
        }
    }

    Ok(())
}

struct CrossEncoderWorker {}

impl llm::PoolingType for CrossEncoderWorker {
    fn pooling_type(&self) -> LlamaPoolingType {
        LlamaPoolingType::Rank
    }
}

impl<'a> Worker<'a, CrossEncoderWorker> {
    pub fn new_crossencoder_worker(
        model: &Arc<LlamaModel>,
        n_ctx: u32,
    ) -> Result<Worker<'_, CrossEncoderWorker>, InitWorkerError> {
        Worker::new_with_type(model, n_ctx, true, CrossEncoderWorker {})
    }

    pub fn get_classification_score(&self) -> Result<f32, CrossEncoderWorkerError> {
        // Cross-encoder models process query+document as single sequence, outputting classification scores.
        // For crossencodering, all tokens have embeddings enabled (logits=true) but only the final token's
        // embedding contains the relevance score. embeddings_seq_ith(0) gets the sequence's embedding
        // vector, and embeddings[0] extracts the classification score from the final token.
        let embeddings = self.ctx.embeddings_seq_ith(0)?;

        if !embeddings.is_empty() {
            Ok(embeddings[0])
        } else {
            Err(CrossEncoderWorkerError::EmptyClassificationHead)
        }
    }

    pub fn rank(
        &mut self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<f32>, CrossEncoderWorkerError> {
        // Get CLS and SEP tokens from the model (CLS = BOS per llama.cpp, the current CLS token is deprecated.)
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let cls = self
            .ctx
            .model
            .token_to_piece(self.ctx.model.token_bos(), &mut decoder, true, None)
            .unwrap_or_else(|_| {
                warn!("Failed to convert BOS/CLS token to string, using fallback");
                "<s>".to_string()
            });

        let sep = self
            .ctx
            .model
            .token_to_piece(self.ctx.model.token_sep(), &mut decoder, true, None)
            .unwrap_or_else(|_| {
                warn!("Failed to convert SEP token to string, using fallback");
                "</s>".to_string()
            });

        let mut scores = Vec::new();
        for document in documents {
            self.reset_context();
            // Format as: [CLS] query [SEP] document [SEP]
            let input = format!("{cls}{query}{sep}{document}{sep}");
            let score = self.read_string(input)?.get_classification_score()?;
            scores.push(score);
        }
        Ok(scores)
    }
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
}
