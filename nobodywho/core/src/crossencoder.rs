use crate::llm;
use crate::llm::Worker;
use llama_cpp_2::context::params::LlamaPoolingType;
use llama_cpp_2::model::LlamaModel;
use tracing::{error};
use std::sync::Arc;

pub struct CrossEncoderHandle {
    msg_tx: std::sync::mpsc::Sender<CrossEncoderMsg>,
}

impl CrossEncoderHandle {
    pub fn new(model: Arc<LlamaModel>, n_ctx: u32) -> Self {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            if let Err(e) = run_worker(model, n_ctx, msg_rx) {
                error!("Crossencoder worker crashed: {}", e)
            }
        });

        Self { msg_tx }
    }

    pub fn rank(&self, query: String, documents: Vec<String>) -> tokio::sync::mpsc::Receiver<Vec<f32>> {
        let (scores_tx, scores_rx) = tokio::sync::mpsc::channel(1);
        let _ = self.msg_tx.send(CrossEncoderMsg::Rank(query, documents, scores_tx));
        scores_rx
    }
}

enum CrossEncoderMsg {
    Rank(String, Vec<String>, tokio::sync::mpsc::Sender<Vec<f32>>),
}

#[derive(Debug, thiserror::Error)]
enum CrossEncoderWorkerError {
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
    msg_rx: std::sync::mpsc::Receiver<CrossEncoderMsg>,
) -> Result<(), CrossEncoderWorkerError> {
    let mut worker_state = Worker::new_crossencoder_worker(&model, n_ctx)?;
    while let Ok(msg) = msg_rx.recv() {
        match msg {
            CrossEncoderMsg::Rank(query, documents, respond) => {
                // Clear context for each crossencodering operation
                worker_state.reset_context();

                let scores = worker_state.rank(query, documents)?;
                
                let _ = respond.blocking_send(scores);
            }
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
    ) -> Result<Worker<'_, CrossEncoderWorker>, llm::InitWorkerError> {
        Worker::new_with_type(model, n_ctx, true, CrossEncoderWorker {})
    }

    pub fn get_classification_score(&self) -> Result<f32, CrossEncoderWorkerError> {
        // Cross-encoder models process query+document as single sequence, outputting classification scores.
        // For crossencodering, all tokens have embeddings enabled (logits=true) but only the final token's
        // embedding contains the relevance score. embeddings_seq_ith(0) gets the sequence's embedding
        // vector, and embeddings[0] extracts the classification score from the final token.
        let embeddings = self.ctx.embeddings_seq_ith(0).map_err(|e| CrossEncoderWorkerError::ClassificationError(e.to_string()))?;
        
        if embeddings.len() >= 1 {
            Ok(embeddings[0])
        } else {
            Err(CrossEncoderWorkerError::ClassificationError("classification head is empty".to_string()))
        }
    }

    pub fn rank(&mut self, query: String, documents: Vec<String>) -> Result<Vec<f32>, CrossEncoderWorkerError> {
        let mut scores = Vec::new();
        for document in documents {
            self.reset_context();
            // TODO: use the cls and sep tokens for this.
            let input = format!("{query}</s><s>{document}</s>");
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
        let mut docs_with_scores: Vec<(String, f32)> = documents.iter().zip(scores.iter()).map(
            |(doc, score)| (doc.clone(), *score)
        ).collect();

        docs_with_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        let mut seen_paris = false;
        for i in 0..3 {
            if docs_with_scores[i].0 == "Paris is the capital of France." {
                seen_paris = true;
            }
        }
        assert!(seen_paris, "`Paris is the capital of France.` is not in top three");

        Ok(())
    }


    
} 
