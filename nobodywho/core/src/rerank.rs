use crate::llm;
use crate::llm::Worker;
use llama_cpp_2::context::params::LlamaPoolingType;
use llama_cpp_2::model::LlamaModel;
use tracing::error;
use std::sync::Arc;

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

    pub fn rank(&self, query: String, documents: Vec<String>) -> tokio::sync::mpsc::Receiver<Vec<f32>> {
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

                let scores = worker_state.rank(query, documents)?;
                
                let _ = respond.blocking_send(scores);
            }
        }
    }
    Ok(())
}

struct RerankerWorker {}

impl llm::PoolingType for RerankerWorker {
    fn pooling_type(&self) -> LlamaPoolingType {
        LlamaPoolingType::Rank
    }
}

impl<'a> Worker<'a, RerankerWorker> {
    pub fn new_reranker_worker(
        model: &Arc<LlamaModel>,
        n_ctx: u32,
    ) -> Result<Worker<'_, RerankerWorker>, llm::InitWorkerError> {
        Worker::new_with_type(model, n_ctx, true, RerankerWorker {})
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

    pub fn rank(&mut self, query: String, documents: Vec<String>) -> Result<Vec<f32>, RerankerWorkerError> {
        let mut scores = Vec::new();
        for document in documents {
            self.reset_context();
            let input = format!("Query: {}\nDocument: {}", query, document);
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
    use rand::{seq::SliceRandom};

    #[test]
    fn test_reranker() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_reranker_model();

        let mut worker = Worker::new_reranker_worker(&model, 1024)?;

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
        let mut docs_with_scores: Vec<(String, f32)> = documents.iter().zip(scores.iter()).map(
            |(doc, score)| (doc.clone(), *score)
        ).collect();

        docs_with_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        // does not really test the accuracy of the the ranker, but rather that it works
        assert_eq!(docs_with_scores[0].0, "Paris is the capital of France.".to_string());

        Ok(())
    }

} 