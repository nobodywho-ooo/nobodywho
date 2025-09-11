#[flutter_rust_bridge::frb(opaque)]
pub struct NobodyWhoModel {
    model: nobodywho::llm::Model,
}

impl NobodyWhoModel {
    #[flutter_rust_bridge::frb(sync)]
    pub fn new(model_path: &str, #[frb(default = true)] use_gpu: bool) -> Self {
        let model = nobodywho::llm::get_model(model_path, use_gpu).expect("TODO: error handling");
        Self { model }
    }
}

#[flutter_rust_bridge::frb(opaque)]
pub struct NobodyWhoChat {
    chat: nobodywho::chat::ChatHandle,
}

impl NobodyWhoChat {
    #[flutter_rust_bridge::frb(sync)]
    pub fn new(model: NobodyWhoModel, system_prompt: String, context_size: u32) -> Self {
        let chat = nobodywho::chat::ChatBuilder::new(model.model)
            .with_system_prompt(system_prompt)
            .with_context_size(context_size)
            .build();
        Self { chat }
    }

    pub async fn say(&self, sink: crate::frb_generated::StreamSink<String>, message: String) -> () {
        let mut stream = self.chat.say_stream(message);
        while let Some(token) = stream.next_token().await {
            sink.add(token).expect("TODO: error handling");
        }
    }
}

// TODO:
// - error handling
// - tools
// - blocking say
// - embeddings
// - cross encoder
// - sampler
