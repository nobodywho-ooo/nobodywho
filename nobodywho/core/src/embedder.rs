use crate::errors::EmbedderWorkerError;
use crate::llm;
use crate::llm::{Model, Worker, WorkerGuard};
use llama_cpp_2::context::params::LlamaPoolingType;
use std::sync::Arc;

/// Embedder for converting text to vectors for semantic search
#[derive(Clone)]
pub struct Embedder {
    async_handle: EmbedderAsync,
}

#[derive(Clone)]
pub struct EmbedderAsync {
    guard: Arc<WorkerGuard<EmbedderMsg>>,
}

impl Embedder {
    pub fn new(model: Arc<Model>, n_ctx: u32) -> Self {
        let async_handle = EmbedderAsync::new(model, n_ctx);
        Self { async_handle }
    }

    /// Embed a single text into a vector
    pub fn embed(&self, text: String) -> Result<Vec<f32>, EmbedderWorkerError> {
        futures::executor::block_on(async { self.async_handle.embed(text).await })
    }

    /// Embed multiple texts (batch)
    pub fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, EmbedderWorkerError> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text)?);
        }
        Ok(results)
    }
}

impl EmbedderAsync {
    pub fn new(model: Arc<Model>, n_ctx: u32) -> Self {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel();

        let join_handle = std::thread::spawn(move || {
            let worker = Worker::new_embedder_worker(&model, n_ctx);
            let mut worker_state = match worker {
                Ok(worker_state) => worker_state,
                Err(errmsg) => {
                    return tracing::error!(error=%errmsg, "Could not set up embedder worker")
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

    pub async fn embed(&self, text: String) -> Result<Vec<f32>, EmbedderWorkerError> {
        let (embeddings_tx, mut embeddings_rx) = tokio::sync::mpsc::channel(1);
        self.guard.send(EmbedderMsg::Embed(text, embeddings_tx));
        embeddings_rx
            .recv()
            .await
            .ok_or(EmbedderWorkerError::NoResponse)?
    }
}

enum EmbedderMsg {
    Embed(
        String,
        tokio::sync::mpsc::Sender<Result<Vec<f32>, EmbedderWorkerError>>,
    ),
}

fn process_worker_msg(worker_state: &mut Worker<'_, EmbedderWorker>, msg: EmbedderMsg) {
    match msg {
        EmbedderMsg::Embed(text, respond) => {
            worker_state.reset_context();
            let result = worker_state.embed(text);
            let _ = respond.blocking_send(result);
        }
    }
}

/// Embedder worker with Mean pooling (like Gecko, Nomic)
struct EmbedderWorker {}

impl llm::PoolingType for EmbedderWorker {
    fn pooling_type(&self) -> LlamaPoolingType {
        LlamaPoolingType::Mean // Mean pooling for embeddings
    }
}

impl<'a> Worker<'a, EmbedderWorker> {
    pub fn new_embedder_worker(
        model: &'a Model,
        n_ctx: u32,
    ) -> Result<Worker<'a, EmbedderWorker>, crate::errors::InitWorkerError> {
        Worker::new_with_type(model, n_ctx, true, EmbedderWorker {})
    }

    pub fn embed(&mut self, text: String) -> Result<Vec<f32>, EmbedderWorkerError> {
        // Process text through model
        self.read_string(text)?;

        // Get embeddings (mean-pooled)
        let embeddings = self.ctx.embeddings_seq_ith(0)?;

        if embeddings.is_empty() {
            return Err(EmbedderWorkerError::EmptyEmbedding);
        }

        Ok(embeddings.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time API test - verifies the embedder API works
    #[test]
    fn test_embedder_api_compiles() {
        // This test verifies that the Embedder API compiles correctly
        // It doesn't run actual inference (would need a model file)

        // These are just compile-time checks
        fn _check_embedder_api() {
            use crate::llm::Model;
            use std::sync::Arc;

            // Check that we can create an embedder (doesn't execute)
            let _create = |model: Arc<Model>| {
                let embedder = Embedder::new(model, 512);
                let _: Result<Vec<f32>, _> = embedder.embed("test".to_string());
                let _: Result<Vec<Vec<f32>>, _> = embedder.embed_batch(vec!["test".to_string()]);
            };
        }
    }

    // Integration test - requires actual embedding model
    // Run with: TEST_EMBEDDINGS_MODEL=/path/to/model.gguf cargo test test_embed_with_model -- --ignored
    #[test]
    #[ignore]
    fn test_embed_with_model() {
        use crate::test_utils::{init_test_tracing, load_embeddings_model};

        init_test_tracing();
        let model = load_embeddings_model();
        let embedder = Embedder::new(model, 512);

        let text = "This is a test sentence for embedding.".to_string();
        let result = embedder.embed(text);

        assert!(result.is_ok(), "Embedding should succeed");
        let embedding = result.unwrap();
        assert!(!embedding.is_empty(), "Embedding should not be empty");

        // Check that embedding values are reasonable
        assert!(
            embedding.iter().all(|&x| x.is_finite()),
            "All values should be finite"
        );

        println!("✓ Embedding dimension: {}", embedding.len());
        println!("✓ Sample values: {:?}", &embedding[0..5]);
    }

    // RAG test with PDF content
    #[test]
    #[ignore]
    fn test_pdf_rag_workflow() {
        use crate::test_utils::{init_test_tracing, load_embeddings_model};

        init_test_tracing();
        println!("\n🚀 Testing RAG Workflow with PDF Content\n");

        // Load embedder
        let model = load_embeddings_model();
        let embedder = Embedder::new(model, 512);

        // PDF chunks from "Agentic AI" playbook
        let documents = vec![
            "Agentic AI generally refers to AI systems that possess the capacity to make autonomous decisions and take actions to achieve specific goals with limited or no direct human intervention.",
            "Autonomy: Agentic AI systems can operate independently, making decisions based on their programming, learning, and environmental inputs.",
            "Goal-oriented behaviour: These AI agents are designed to pursue specific objectives, optimising their actions to achieve the desired outcomes.",
            "Environment interaction: An agentic AI interacts with its surroundings, perceiving changes and adapting its strategies accordingly.",
            "Learning capability: Many agentic AI systems employ machine learning or reinforcement learning techniques to improve their performance over time.",
            "GenAI is being recognised as a game-changer for innovation in the region, empowering enterprises by automating routine tasks, enhancing customer experiences and assisting in critical decision-making.",
        ];

        println!("📚 Embedding {} documents...", documents.len());

        // Embed all documents
        let embeddings = embedder
            .embed_batch(documents.iter().map(|s| s.to_string()).collect())
            .expect("Batch embedding should succeed");

        println!(
            "✅ Created {} embeddings ({}D each)\n",
            embeddings.len(),
            embeddings[0].len()
        );

        // Test queries
        let queries = vec![
            "What is agentic AI?",
            "How do AI agents learn?",
            "What are the key characteristics of autonomous AI?",
        ];

        for query in queries {
            println!("🔍 Query: \"{}\"", query);

            // Embed query
            let query_embedding = embedder
                .embed(query.to_string())
                .expect("Query embedding should succeed");

            // Calculate cosine similarity with all documents
            let mut scored_docs: Vec<(usize, f32)> = embeddings
                .iter()
                .enumerate()
                .map(|(idx, doc_emb)| {
                    let similarity = cosine_similarity(&query_embedding, doc_emb);
                    (idx, similarity)
                })
                .collect();

            // Sort by similarity (highest first)
            scored_docs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

            // Show top 3 results
            println!("   Top 3 Results:");
            for (rank, (idx, score)) in scored_docs.iter().take(3).enumerate() {
                println!(
                    "   {}. [Score: {:.3}] {}",
                    rank + 1,
                    score,
                    &documents[*idx][..80.min(documents[*idx].len())]
                );
            }
            println!();
        }

        println!("✨ RAG workflow completed successfully!");
    }

    // Helper: Cosine similarity
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        assert_eq!(a.len(), b.len());

        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if magnitude_a == 0.0 || magnitude_b == 0.0 {
            return 0.0;
        }

        dot_product / (magnitude_a * magnitude_b)
    }
}
