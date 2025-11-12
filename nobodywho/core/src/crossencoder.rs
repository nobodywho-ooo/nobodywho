use crate::errors::{CrossEncoderWorkerError, InitWorkerError};
use crate::llm;
use crate::llm::Worker;
use llama_cpp_2::context::params::LlamaPoolingType;
use llama_cpp_2::model::{LlamaModel, Special};
use std::sync::Arc;
use tracing::{error, warn};

pub struct CrossEncoderHandle {
    msg_tx: std::sync::mpsc::Sender<CrossEncoderMsg>,
}

impl CrossEncoderHandle {
    pub fn new(model: Arc<LlamaModel>, n_ctx: u32) -> Self {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            let Ok(mut worker_state) = Worker::new_crossencoder_worker(&model, n_ctx) else {
                return error!("Could not set up the worker initial state");
            };

            while let Ok(msg) = msg_rx.recv() {
                if let Err(e) = process_worker_msg(&mut worker_state, msg) {
                    return error!("Cross-encoder worker crashed: {e}");
                }
            }
        });

        Self { msg_tx }
    }

    pub fn rank(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> tokio::sync::mpsc::Receiver<Vec<f32>> {
        let (scores_tx, scores_rx) = tokio::sync::mpsc::channel(1);
        let _ = self
            .msg_tx
            .send(CrossEncoderMsg::Rank(query, documents, scores_tx));
        scores_rx
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
        let embeddings = self
            .ctx
            .embeddings_seq_ith(0)
            .map_err(|e| CrossEncoderWorkerError::Classification(e.to_string()))?;

        if !embeddings.is_empty() {
            Ok(embeddings[0])
        } else {
            Err(CrossEncoderWorkerError::Classification(
                "classification head is empty".to_string(),
            ))
        }
    }

    pub fn rank(
        &mut self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<f32>, CrossEncoderWorkerError> {
        // Get CLS and SEP tokens from the model (CLS = BOS per llama.cpp, the current CLS token is deprecated.)
        let cls = self
            .ctx
            .model
            .token_to_str(self.ctx.model.token_bos(), Special::Tokenize)
            .unwrap_or_else(|_| {
                warn!("Failed to convert BOS/CLS token to string, using fallback");
                "<s>".to_string()
            });

        let sep = self
            .ctx
            .model
            .token_to_str(self.ctx.model.token_sep(), Special::Tokenize)
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
    use rand::seq::SliceRandom;

    #[test]
    fn test_crossencoder() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_crossencoder_model();

        let mut worker = Worker::new_crossencoder_worker(&model, 1024)?;

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
        let mut rng = rand::rng();
        documents.shuffle(&mut rng);

        let scores = worker.rank(query, documents.clone())?;
        // The highest score for this should be the phrase  Paris is the capital of France.
        let mut docs_with_scores: Vec<(String, f32)> = documents
            .iter()
            .zip(scores.iter())
            .map(|(doc, score)| (doc.clone(), *score))
            .collect();

        docs_with_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        let mut seen_paris = false;
        for i in 0..3 {
            if docs_with_scores[i].0 == "Paris is the capital of France." {
                seen_paris = true;
            }
        }
        assert!(
            seen_paris,
            "`Paris is the capital of France.` is not in top three"
        );

        Ok(())
    }
}
