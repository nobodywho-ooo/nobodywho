use nobodywho_rust::{Chat, Model, Prompt};
use std::path::PathBuf;

fn vision_model_path() -> String {
    std::env::var("TEST_VISION_MODEL").unwrap_or_else(|_| "vision-model.gguf".to_string())
}

fn mmproj_path() -> String {
    std::env::var("TEST_MMPROJ").unwrap_or_else(|_| "mmproj.gguf".to_string())
}

/// Images live next to the Python tests to avoid duplicating assets.
fn img(name: &str) -> PathBuf {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    root.join("../python/tests/img").join(name)
}

/// Returns `None` and prints a skip message when the required env vars aren't set.
fn try_load_vision_model() -> Option<Model> {
    if std::env::var("TEST_VISION_MODEL").is_err() || std::env::var("TEST_MMPROJ").is_err() {
        eprintln!("SKIP: TEST_VISION_MODEL or TEST_MMPROJ not set");
        return None;
    }
    Some(
        Model::builder(vision_model_path())
            .with_mmproj(mmproj_path())
            .build()
            .expect("failed to load vision model"),
    )
}

fn make_vision_chat(model: &Model) -> Chat {
    Chat::builder(model)
        .with_system_prompt("You are a helpful assistant.")
        .with_template_variable("enable_thinking", false)
        .build()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn test_image_description() {
    let Some(model) = try_load_vision_model() else {
        return;
    };
    let chat = make_vision_chat(&model);

    let mut prompt = Prompt::new();
    prompt.push_text(
        "What animal is in this image? Short answer. Focus on the species, not the age or breed."
            .to_string(),
    );
    prompt.push_image(img("penguin.png").as_ref());

    let response = chat.ask(prompt).completed().unwrap();
    assert!(
        response.to_lowercase().contains("penguin"),
        "got: {response}"
    );
}

#[test]
fn test_multiple_images() {
    let Some(model) = try_load_vision_model() else {
        return;
    };
    let chat = make_vision_chat(&model);

    let mut prompt = Prompt::new();
    prompt.push_image(img("penguin.png").as_ref());
    prompt.push_image(img("dog.png").as_ref());
    prompt.push_text(
        "What animals are in these images? Short answer. Focus on the species, not the age or breed."
            .to_string(),
    );

    let response = chat.ask(prompt).completed().unwrap();
    assert!(
        response.to_lowercase().contains("penguin"),
        "got: {response}"
    );
    assert!(response.to_lowercase().contains("dog"), "got: {response}");
}

#[test]
fn test_multiple_images_interleaved() {
    let Some(model) = try_load_vision_model() else {
        return;
    };
    let chat = make_vision_chat(&model);

    let mut prompt = Prompt::new();
    prompt.push_text("What animal is in the first image?".to_string());
    prompt.push_image(img("penguin.png").as_ref());
    prompt.push_text("What animal is in the second image?".to_string());
    prompt.push_image(img("dog.png").as_ref());
    prompt.push_text("Short answer. Focus on the species, not the age or breed.".to_string());

    let response = chat.ask(prompt).completed().unwrap();
    assert!(
        response.to_lowercase().contains("penguin"),
        "got: {response}"
    );
    assert!(response.to_lowercase().contains("dog"), "got: {response}");
}

#[test]
fn test_image_recollection() {
    let Some(model) = try_load_vision_model() else {
        return;
    };
    let chat = make_vision_chat(&model);

    let mut prompt = Prompt::new();
    prompt.push_text(
        "What animal is in this image? Short answer. Focus on the species, not the age or breed."
            .to_string(),
    );
    prompt.push_image(img("dog.png").as_ref());

    let response = chat.ask(prompt).completed().unwrap();
    assert!(response.to_lowercase().contains("dog"), "got: {response}");

    // follow-up question about the same image, already in history
    let response2 = chat
        .ask("What is the color of the flowers in the background of the image? Short answer.")
        .completed()
        .unwrap();
    assert!(
        response2.to_lowercase().contains("orange"),
        "got: {response2}"
    );
}
