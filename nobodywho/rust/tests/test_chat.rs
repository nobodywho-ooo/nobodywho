use nobodywho_rust::{
    cosine_similarity, Chat, ChatAsync, CrossEncoder, CrossEncoderAsync, Encoder, EncoderAsync,
    Message, Model, Role,
};
use std::collections::HashMap;

fn assert_history_matches(history: &[Message], expected: &[(&str, &str)]) {
    assert_eq!(history.len(), expected.len(), "history length mismatch");
    for (msg, (expected_role, expected_content)) in history.iter().zip(expected) {
        let (role, content) = match msg {
            Message::Message { role, content, .. } => (role, content.as_str()),
            _ => panic!("expected a plain Message variant"),
        };
        let role_str = match role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
            Role::Tool => "tool",
        };
        assert_eq!(role_str, *expected_role);
        assert_eq!(content, *expected_content);
    }
}

fn model_path() -> String {
    std::env::var("TEST_MODEL").unwrap_or_else(|_| "model.gguf".to_string())
}

fn embeddings_model_path() -> String {
    std::env::var("TEST_EMBEDDINGS_MODEL").unwrap_or_else(|_| "embeddings.gguf".to_string())
}

fn crossencoder_model_path() -> String {
    std::env::var("TEST_CROSSENCODER_MODEL").unwrap_or_else(|_| "crossencoder.gguf".to_string())
}

fn load_model() -> Model {
    Model::builder(model_path())
        .build()
        .expect("failed to load model")
}

fn load_embeddings_model() -> Model {
    Model::builder(embeddings_model_path())
        .use_gpu(false)
        .build()
        .expect("failed to load embeddings model")
}

fn load_crossencoder_model() -> Model {
    Model::builder(crossencoder_model_path())
        .use_gpu(false)
        .build()
        .expect("failed to load crossencoder model")
}

fn make_chat(model: &Model) -> Chat {
    Chat::builder(model)
        .with_system_prompt("You are a helpful assistant.")
        .with_template_variable("enable_thinking", false)
        .build()
}

fn make_chat_async(model: &Model) -> ChatAsync {
    Chat::builder(model)
        .with_system_prompt("You are a helpful assistant.")
        .with_template_variable("enable_thinking", false)
        .build_async()
}

// ── Model loading ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_async_model_loading() {
    let model = Model::builder(model_path())
        .build_async()
        .await
        .expect("async model load failed");
    // just verify it loaded without panic
    let _ = model;
}

// ── Sync chat ─────────────────────────────────────────────────────────────────

#[test]
fn test_blocking_completed() {
    let model = load_model();
    let chat = make_chat(&model);
    let response = chat
        .ask("What is the capital of Denmark?")
        .completed()
        .unwrap();
    assert!(
        response.to_lowercase().contains("copenhagen"),
        "got: {response}"
    );
}

#[test]
fn test_sync_streaming() {
    let model = load_model();
    let chat = make_chat(&model);
    let mut stream = chat.ask("What is the capital of Denmark?");
    let mut response = String::new();
    while let Some(token) = stream.next_token() {
        assert!(!token.is_empty());
        response.push_str(&token);
    }
    assert!(
        response.to_lowercase().contains("copenhagen"),
        "got: {response}"
    );
}

#[test]
fn test_multiple_prompts_sync() {
    let model = load_model();
    let chat = make_chat(&model);
    for prompt in ["Hello", "What is 2+2?", "Goodbye"] {
        let response = chat.ask(prompt).completed().unwrap();
        assert!(!response.is_empty());
    }
}

// ── Async chat ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_async_streaming() {
    let model = load_model();
    let chat = make_chat_async(&model);
    let mut stream = chat.ask("What is the capital of Denmark?");
    let mut response = String::new();
    while let Some(token) = stream.next_token().await {
        response.push_str(&token);
    }
    assert!(
        response.to_lowercase().contains("copenhagen"),
        "got: {response}"
    );
}

#[tokio::test]
async fn test_async_completed() {
    let model = load_model();
    let chat = make_chat_async(&model);
    let response = chat
        .ask("What is the capital of Denmark?")
        .completed()
        .await
        .unwrap();
    assert!(
        response.to_lowercase().contains("copenhagen"),
        "got: {response}"
    );
}

#[tokio::test]
async fn test_multiple_prompts_async() {
    let model = load_model();
    let chat = make_chat_async(&model);
    for prompt in ["Hello", "What is 2+2?", "Goodbye"] {
        let response = chat.ask(prompt).completed().await.unwrap();
        assert!(!response.is_empty());
    }
}

// ── History ───────────────────────────────────────────────────────────────────

#[test]
fn test_set_and_get_chat_history() {
    let model = load_model();
    let chat = make_chat(&model);
    let history = vec![
        Message::Message {
            role: Role::User,
            content: "What's 2 + 2?".to_string(),
            assets: vec![],
        },
        Message::Message {
            role: Role::Assistant,
            content: "2 + 2 = 4".to_string(),
            assets: vec![],
        },
    ];
    chat.set_chat_history(history).unwrap();
    let retrieved = chat.get_chat_history().unwrap();
    assert_history_matches(
        &retrieved,
        &[("user", "What's 2 + 2?"), ("assistant", "2 + 2 = 4")],
    );
}

#[tokio::test]
async fn test_async_set_and_get_chat_history() {
    let model = load_model();
    let chat = make_chat_async(&model);
    let history = vec![
        Message::Message {
            role: Role::User,
            content: "What's 2 + 2?".to_string(),
            assets: vec![],
        },
        Message::Message {
            role: Role::Assistant,
            content: "2 + 2 = 4".to_string(),
            assets: vec![],
        },
    ];
    chat.set_chat_history(history).await.unwrap();
    let retrieved = chat.get_chat_history().await.unwrap();
    assert_history_matches(
        &retrieved,
        &[("user", "What's 2 + 2?"), ("assistant", "2 + 2 = 4")],
    );
}

#[test]
fn test_reset_chat_history() {
    let model = load_model();
    let chat = make_chat(&model);
    chat.ask("My name is Bob.").completed().unwrap();
    chat.set_chat_history(vec![]).unwrap();
    let response = chat.ask("What did I just tell you?").completed().unwrap();
    assert!(!response.to_lowercase().contains("bob"), "got: {response}");
}

// ── System prompt ─────────────────────────────────────────────────────────────

#[test]
fn test_set_system_prompt() {
    let model = load_model();
    let chat = make_chat(&model);

    chat.ask("My name is Alice.").completed().unwrap();

    chat.set_system_prompt(Some(
        "You must respond only with the word 'BEEP' repeated.".to_string(),
    ))
    .unwrap();
    chat.reset_history().unwrap();

    let response = chat.ask("Hello, how are you?").completed().unwrap();
    assert!(response.to_uppercase().contains("BEEP"), "got: {response}");
}

// ── Stop generation ───────────────────────────────────────────────────────────

#[test]
fn test_stop_generation() {
    let model = load_model();
    let chat = make_chat(&model);
    let mut stream = chat.ask("Count from 1 to 100 slowly, one number per line.");

    let mut tokens = Vec::new();
    for _ in 0..6 {
        match stream.next_token() {
            Some(t) => tokens.push(t),
            None => break,
        }
    }
    chat.stop_generation();

    while let Some(t) = stream.next_token() {
        tokens.push(t);
    }

    let full = tokens.concat();
    assert!(
        full.len() < 200,
        "generation should have been stopped early, got {} chars",
        full.len()
    );
}

// ── Sampler ───────────────────────────────────────────────────────────────────

#[test]
fn test_template_variables() {
    let model = load_model();
    let mut vars = HashMap::new();
    vars.insert("enable_thinking".to_string(), false);
    let chat = Chat::builder(&model).with_template_variables(vars).build();
    let vars_back = chat.get_template_variables().unwrap();
    assert_eq!(vars_back.get("enable_thinking"), Some(&false));
}

// ── Encoder ───────────────────────────────────────────────────────────────────

#[test]
fn test_encoder_sync() {
    let model = load_embeddings_model();
    let encoder = Encoder::new(&model, 1024);
    let embedding = encoder
        .encode("Test text for embedding.".to_string())
        .unwrap();
    assert!(!embedding.is_empty());
    assert!(embedding.iter().all(|x| x.is_finite()));
}

#[tokio::test]
async fn test_encoder_async() {
    let model = load_embeddings_model();
    let encoder = EncoderAsync::new(&model, 1024);
    let embedding = encoder
        .encode("Test text for embedding.".to_string())
        .await
        .unwrap();
    assert!(!embedding.is_empty());
    assert!(embedding.iter().all(|x| x.is_finite()));
}

#[test]
fn test_cosine_similarity() {
    let vec1 = vec![1.0f32, 2.0, 3.0];
    let vec2 = vec![4.0f32, 5.0, 6.0];
    let sim = cosine_similarity(&vec1, &vec2);
    assert!(sim.is_finite());

    let self_sim = cosine_similarity(&vec1, &vec1);
    assert!(
        (self_sim - 1.0).abs() < 0.001,
        "self-similarity should be ~1.0, got {self_sim}"
    );
}

// ── CrossEncoder ──────────────────────────────────────────────────────────────

#[test]
fn test_crossencoder_rank_sync() {
    let model = load_crossencoder_model();
    let ce = CrossEncoder::new(&model, 4096);
    let query = "What is the capital of France?".to_string();
    let documents = vec![
        "Paris is the capital of France.".to_string(),
        "Berlin is the capital of Germany.".to_string(),
        "The weather is nice today.".to_string(),
    ];
    let scores = ce.rank(query, documents.clone()).unwrap();
    assert_eq!(scores.len(), documents.len());
    assert!(scores.iter().all(|s| s.is_finite()));
}

#[tokio::test]
async fn test_crossencoder_rank_async() {
    let model = load_crossencoder_model();
    let ce = CrossEncoderAsync::new(&model, 4096);
    let query = "What is the capital of France?".to_string();
    let documents = vec![
        "Paris is the capital of France.".to_string(),
        "Berlin is the capital of Germany.".to_string(),
    ];
    let scores = ce.rank(query, documents.clone()).await.unwrap();
    assert_eq!(scores.len(), documents.len());
    assert!(scores.iter().all(|s| s.is_finite()));
}

#[test]
fn test_crossencoder_rank_and_sort_sync() {
    let model = load_crossencoder_model();
    let ce = CrossEncoder::new(&model, 4096);
    let query = "What is the capital of France?".to_string();
    let documents = vec![
        "Paris is the capital of France.".to_string(),
        "Berlin is the capital of Germany.".to_string(),
        "The weather is nice today.".to_string(),
    ];
    let ranked = ce.rank_and_sort(query, documents.clone()).unwrap();
    assert_eq!(ranked.len(), documents.len());
    for (doc, score) in &ranked {
        assert!(documents.contains(doc));
        assert!(score.is_finite());
    }
    // highest-scoring doc should be the Paris one
    assert!(
        ranked[0].0.contains("Paris"),
        "expected Paris first, got: {}",
        ranked[0].0
    );
}

#[tokio::test]
async fn test_crossencoder_rank_and_sort_async() {
    let model = load_crossencoder_model();
    let ce = CrossEncoderAsync::new(&model, 4096);
    let query = "What is the capital of France?".to_string();
    let documents = vec![
        "Paris is the capital of France.".to_string(),
        "Berlin is the capital of Germany.".to_string(),
    ];
    let ranked = ce.rank_and_sort(query, documents.clone()).await.unwrap();
    assert_eq!(ranked.len(), documents.len());
    for (doc, score) in &ranked {
        assert!(documents.contains(doc));
        assert!(score.is_finite());
    }
}
