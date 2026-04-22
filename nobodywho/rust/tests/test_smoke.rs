use nobodywho_rust::{
    Chat, ChatAsync, CrossEncoder, CrossEncoderAsync, Encoder, EncoderAsync, Model,
};

fn chat_model() -> Model {
    let path = std::env::var("TEST_MODEL").unwrap_or_else(|_| "model.gguf".to_string());
    Model::builder(path)
        .build()
        .expect("failed to load chat model")
}

fn embeddings_model() -> Model {
    let path =
        std::env::var("TEST_EMBEDDINGS_MODEL").unwrap_or_else(|_| "embeddings.gguf".to_string());
    Model::builder(path)
        .use_gpu(false)
        .build()
        .expect("failed to load embeddings model")
}

fn crossencoder_model() -> Model {
    let path = std::env::var("TEST_CROSSENCODER_MODEL")
        .unwrap_or_else(|_| "crossencoder.gguf".to_string());
    Model::builder(path)
        .use_gpu(false)
        .build()
        .expect("failed to load crossencoder model")
}

#[test]
fn test_create_chat() {
    let model = chat_model();
    let _chat = Chat::builder(&model).build();
}

#[test]
fn test_create_chat_async() {
    let model = chat_model();
    let _chat = Chat::builder(&model).build_async();
}

#[test]
fn test_create_encoder() {
    let model = embeddings_model();
    let _encoder = Encoder::new(&model, 512);
}

#[test]
fn test_create_encoder_async() {
    let model = embeddings_model();
    let _encoder = EncoderAsync::new(&model, 512);
}

#[test]
fn test_create_crossencoder() {
    let model = crossencoder_model();
    let _ce = CrossEncoder::new(&model, 512);
}

#[tokio::test]
async fn test_create_crossencoder_async() {
    let model = crossencoder_model();
    let _ce = CrossEncoderAsync::new(&model, 512);
}

#[test]
fn test_chat_simple_question() {
    let model = chat_model();
    let chat = Chat::builder(&model)
        .with_system_prompt("You are a helpful assistant.")
        .build();
    let response = chat.ask("What is 1+1?").completed().unwrap();
    assert!(!response.is_empty(), "expected a non-empty response");
}
