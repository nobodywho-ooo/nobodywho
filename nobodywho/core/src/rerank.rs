use crate::llm;
use crate::llm::Worker;
use llama_cpp_2::model::LlamaModel;
use tracing::error;

use std::sync::Arc;

// RerankerHandle - for parallelism

pub struct RerankerHandle {
    msg_tx: std::sync::mpsc::Sender<RerankerMsg>,
}

impl RerankerHandle {
    pub fn new(model: Arc<LlamaModel>, n_ctx: u32) -> Self {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            if let Err(e) = run_worker(model, n_ctx, msg_rx) {
                error!("Reranker worker crashed: {}", e)
            }
        });

        Self { msg_tx }
    }

    pub fn rerank(&self, query: String, documents: Vec<String>) -> tokio::sync::mpsc::Receiver<Vec<f32>> {
        let (scores_tx, scores_rx) = tokio::sync::mpsc::channel(1);
        let _ = self.msg_tx.send(RerankerMsg::Rerank(query, documents, scores_tx));
        scores_rx
    }
}

enum RerankerMsg {
    Rerank(String, Vec<String>, tokio::sync::mpsc::Sender<Vec<f32>>),
}

#[derive(Debug, thiserror::Error)]
enum RerankerWorkerError {
    #[error("Error initializing worker: {0}")]
    InitWorkerError(#[from] llm::InitWorkerError),

    #[error("Error reading string: {0}")]
    ReadError(#[from] llm::ReadError),

    #[error("Error getting classification score: {0}")]
    ClassificationError(String),
}

fn run_worker(
    model: Arc<LlamaModel>,
    n_ctx: u32,
    msg_rx: std::sync::mpsc::Receiver<RerankerMsg>,
) -> Result<(), RerankerWorkerError> {
    let mut worker_state = Worker::new_reranker_worker(&model, n_ctx)?;
    while let Ok(msg) = msg_rx.recv() {
        match msg {
            RerankerMsg::Rerank(query, documents, respond) => {
                // Clear context for each reranking operation
                worker_state.reset_context();

                let mut scores = Vec::new();
                for document in documents {
                    // Format as query + document pair for cross-encoder
                    let input = format!("Query: {}\nDocument: {}", query, document);
                    let score = worker_state.read_string(input)?.get_classification_score()?;
                    scores.push(score);
                }
                let _ = respond.blocking_send(scores);
            }
        }
    }
    Ok(())
}

struct RerankerWorker {}

impl<'a> Worker<'a, RerankerWorker> {
    pub fn new_reranker_worker(
        model: &Arc<LlamaModel>,
        n_ctx: u32,
    ) -> Result<Worker<'_, RerankerWorker>, llm::InitWorkerError> {
        Worker::new_with_type(model, n_ctx, true, true, RerankerWorker {})
    }

    pub fn get_classification_score(&self) -> Result<f32, RerankerWorkerError> {

        // Cross-encoder models process query+document as single sequence, outputting classification scores.
        // For reranking, all tokens have embeddings enabled (logits=true) but only the final token's
        // embedding contains the relevance score. embeddings_seq_ith(0) gets the sequence's embedding
        // vector, and embeddings[0] extracts the classification score from the final token.
        let embeddings = self.ctx.embeddings_seq_ith(0).map_err(|e| RerankerWorkerError::ClassificationError(e.to_string()))?;
        
        if embeddings.len() >= 1 {
            Ok(embeddings[0])
        } else {
            Err(RerankerWorkerError::ClassificationError("classification head is empty".to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils;

    #[test]
    fn test_reranker() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_reranker_model();

        let mut worker = Worker::new_reranker_worker(&model, 1024)?;

        let query = "What is the capital of France?";
        let documents = vec![
            "Paris is the capital of France.".to_string(),
            "The Eiffel Tower is a famous landmark in the capital of France.".to_string(),
            "France is a country in Europe.".to_string(),
        ];

        let mut scores = Vec::new();
        for document in &documents {
            let input = format!("Query: {}\nDocument: {}", query, document);
            let score = worker.read_string(input)?.get_classification_score()?;
            scores.push(score);
        }

        // The first document should have the highest relevance score
        assert!(
            scores[0] > scores[1] && scores[0] > scores[2],
            "Paris document should be most relevant to capital query"
        );

        // All scores should be reasonable (between 0 and 1 for probability scores)
        for score in &scores {
            assert!(*score >= 0.0 && *score <= 1.0, "Score {} should be between 0 and 1", score);
        }

        Ok(())
    }

    #[test]
    fn test_reranker_deterministic() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_reranker_model();
        let mut worker = Worker::new_reranker_worker(&model, 1024)?;
        
        let query = "What is machine learning?";
        let document = "Machine learning is a subset of artificial intelligence.";
        let input = format!("Query: {}\nDocument: {}", query, document);
        
        let first_score = worker.read_string(input.clone())?.get_classification_score()?;
        
        worker.reset_context();
        
        let second_score = worker.read_string(input)?.get_classification_score()?;
        
        assert_eq!(
            first_score,
            second_score,
            "Same input should produce identical classification scores."
        );
        
        Ok(())
    }
} 