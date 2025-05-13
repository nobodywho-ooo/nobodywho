mod sampler_resource;

use godot::classes::{INode, ProjectSettings};
use godot::prelude::*;
use nobodywho::{llm, sampler_config};

use crate::sampler_resource::NobodyWhoSampler;

struct NobodyWhoExtension;

#[gdextension]
unsafe impl ExtensionLibrary for NobodyWhoExtension {}

#[derive(GodotClass)]
#[class(base=Node)]
/// The model node is used to load the model, currently only GGUF models are supported.
///
/// If you dont know what model to use, we would suggest checking out https://huggingface.co/spaces/k-mktr/gpu-poor-llm-arena
struct NobodyWhoModel {
    #[export(file = "*.gguf")]
    model_path: GString,

    #[export]
    use_gpu_if_available: bool,

    model: Option<llm::Model>,
}

#[godot_api]
impl INode for NobodyWhoModel {
    fn init(_base: Base<Node>) -> Self {
        // default values to show in godot editor
        let model_path: String = "model.gguf".into();

        Self {
            model_path: model_path.into(),
            use_gpu_if_available: true,
            model: None,
        }
    }
}

impl NobodyWhoModel {
    // memoized model loader
    fn get_model(&mut self) -> Result<llm::Model, llm::LoadModelError> {
        if let Some(model) = &self.model {
            return Ok(model.clone());
        }

        let project_settings = ProjectSettings::singleton();
        let model_path_string: String = project_settings
            .globalize_path(&self.model_path.clone())
            .into();

        match llm::get_model(model_path_string.as_str(), self.use_gpu_if_available) {
            Ok(model) => {
                self.model = Some(model.clone());
                Ok(model.clone())
            }
            Err(err) => {
                godot_error!("Could not load model: {:?}", err.to_string());
                Err(err)
            }
        }
    }
}

#[derive(GodotClass)]
#[class(base=Node)]
/// NobodyWhoChat is the main node for interacting with the LLM. It functions as a chat, and can be used to send and receive messages.
///
/// The chat node is used to start a new context to send and receive messages (multiple contexts can be used at the same time with the same model).
/// It requires a call to `start_worker()` before it can be used. If you do not call it, the chat will start the worker when you send the first message.
///
/// Example:
///
/// ```
/// extends NobodyWhoChat
///
/// func _ready():
///     # configure node
///     self.model_node = get_node("../ChatModel")
///     self.system_prompt = "You are an evil wizard. Always try to curse anyone who talks to you."
///
///     # say something
///     say("Hi there! Who are you?")
///
///     # wait for the response
///     var response = await response_finished
///     print("Got response: " + response)
///
///     # in this example we just use the `response_finished` signal to get the complete response
///     # in real-world-use you definitely want to connect `response_updated`, which gives one word at a time
///     # the whole interaction feels *much* smoother if you stream the response out word-by-word.
/// ```
///
struct NobodyWhoChat {
    #[export]
    /// The model node for the chat.
    model_node: Option<Gd<NobodyWhoModel>>,

    #[export]
    /// The sampler configuration for the chat.
    sampler: Option<Gd<NobodyWhoSampler>>,

    #[export]
    #[var(hint = MULTILINE_TEXT)]
    /// The system prompt for the chat, this is the basic instructions for the LLM's behavior.
    system_prompt: GString,

    #[export]
    /// Stop tokens to stop generation at these specified tokens.
    stop_words: PackedStringArray,

    #[export]
    /// This is the maximum number of tokens that can be stored in the chat history. It will delete information from the chat history if it exceeds this limit.
    /// Higher values use more VRAM, but allow for longer "short term memory" for the LLM.
    context_length: u32,

    chat_handle: Option<nobodywho::chat::ChatHandle>,

    base: Base<Node>,
}

#[godot_api]
impl INode for NobodyWhoChat {
    fn init(base: Base<Node>) -> Self {
        Self {
            // config
            model_node: None,
            sampler: None,
            system_prompt: "".into(),
            stop_words: PackedStringArray::new(),
            context_length: 4096,
            chat_handle: None,

            base,
        }
    }
}

#[godot_api]
impl NobodyWhoChat {
    fn get_model(&mut self) -> Result<llm::Model, GString> {
        let gd_model_node = self.model_node.as_mut().ok_or("Model node was not set")?;
        let mut nobody_model = gd_model_node.bind_mut();
        let model: llm::Model = nobody_model.get_model().map_err(|e| e.to_string())?;

        Ok(model)
    }

    fn get_sampler_config(&mut self) -> sampler_config::SamplerConfig {
        if let Some(gd_sampler) = self.sampler.as_mut() {
            let nobody_sampler: GdRef<NobodyWhoSampler> = gd_sampler.bind();
            nobody_sampler.sampler_config.clone()
        } else {
            sampler_config::SamplerConfig::default()
        }
    }

    #[func]
    /// Starts the LLM worker thread. This is required before you can send messages to the LLM.
    /// This fuction is blocking and can be a bit slow, so you may want to be strategic about when you call it.
    fn start_worker(&mut self) {
        let mut result = || -> Result<(), String> {
            let model = self.get_model()?;
            self.chat_handle = Some(nobodywho::chat::ChatHandle::new(
                model,
                self.context_length,
                self.system_prompt.to_string(),
            ));
            Ok(())
        };

        // run it and show error in godot if it fails
        if let Err(msg) = result() {
            godot_error!("Error running model: {}", msg);
        }
    }

    #[func]
    /// Sends a message to the LLM.
    /// This will start the inference process. meaning you can also listen on the `response_updated` and `response_finished` signals to get the response.
    fn say(&mut self, message: String) {
        let sampler = self.get_sampler_config();
        if let Some(chat_handle) = self.chat_handle.as_mut() {
            let stop_words = self
                .stop_words
                .to_vec()
                .into_iter()
                .map(|g| g.to_string())
                .collect();
            let mut generation_channel = chat_handle.say(message, sampler, stop_words);

            let mut emit_node = self.to_gd();
            godot::task::spawn(async move {
                while let Some(out) = generation_channel.recv().await {
                    match out {
                        nobodywho::llm::WriteOutput::Token(tok) => emit_node
                            .signals()
                            .response_updated()
                            .emit(&GString::from(tok)),
                        nobodywho::llm::WriteOutput::Done(resp) => emit_node
                            .signals()
                            .response_finished()
                            .emit(&GString::from(resp)),
                    }
                }
            });
        } else {
            godot_warn!("Worker was not started yet, starting now... You may want to call `start_worker()` ahead of time to avoid waiting.");
            self.start_worker();
            self.say(message);
        }
    }

    #[func]
    fn reset_context(&mut self) {
        if let Some(chat_handle) = &self.chat_handle {
            chat_handle.reset_chat(self.system_prompt.to_string());
        } else {
            godot_error!("Attempted to reset context, but no worker is running. Doing nothing.");
        }
    }

    #[signal]
    /// Triggered when a new token is received from the LLM. Returns the new token as a string.
    /// It is strongly recommended to connect to this signal, and display the text output as it is
    /// being generated. This makes for a much nicer user experience.
    fn response_updated(new_token: GString);

    #[signal]
    /// Triggered when the LLM has finished generating the response. Returns the full response as a string.
    fn response_finished(response: GString);
}

#[derive(GodotClass)]
#[class(base=Node)]
/// The Embedding node is used to compare text. This is useful for detecting whether the user said
/// something specific, without having to match on literal keywords or sentences.
///
/// This is done by embedding the text into a vector space and then comparing the cosine similarity between the vectors.
///
/// A good example of this would be to check if a user signals an action like "I'd like to buy the red potion". The following sentences will have high similarity:
/// - Give me the potion that is red
/// - I'd like the red one, please.
/// - Hand me the flask of scarlet hue.
///
/// Meaning you can trigger a "sell red potion" task based on natural language, without requiring a speciific formulation.
/// It can of course be used for all sorts of tasks.
///
/// It requires a "NobodyWhoModel" node to be set with a GGUF model capable of generating embeddings.
/// Example:
///
/// ```
/// extends NobodyWhoEmbedding
///
/// func _ready():
///     # configure node
///     self.model_node = get_node(“../EmbeddingModel”)
///
///     # generate some embeddings
///     embed(“The dragon is on the hill.”)
///     var dragon_hill_embd = await self.embedding_finished
///
///     embed(“The dragon is hungry for humans.”)
///     var dragon_hungry_embd = await self.embedding_finished
///
///     embed(“This does not matter.”)
///     var irrelevant_embd = await self.embedding_finished
///
///     # test similarity,
///     # here we show that two embeddings will have high similarity, if they mean similar things
///     var low_similarity = cosine_similarity(irrelevant_embd, dragon_hill_embd)
///     var high_similarity = cosine_similarity(dragon_hill_embd, dragon_hungry_embd)
///     assert(low_similarity < high_similarity)
/// ```
///
struct NobodyWhoEmbedding {
    #[export]
    /// The model node for the embedding.
    model_node: Option<Gd<NobodyWhoModel>>,
    embed_handle: Option<nobodywho::embed::EmbeddingsHandle>,
    base: Base<Node>,
}

#[godot_api]
impl INode for NobodyWhoEmbedding {
    fn init(base: Base<Node>) -> Self {
        Self {
            model_node: None,
            embed_handle: None,
            base,
        }
    }
}

#[godot_api]
impl NobodyWhoEmbedding {
    #[signal]
    /// Triggered when the embedding has finished. Returns the embedding as a PackedFloat32Array.
    fn embedding_finished(embedding: PackedFloat32Array);

    fn get_model(&mut self) -> Result<llm::Model, String> {
        let gd_model_node = self.model_node.as_mut().ok_or("Model node was not set")?;
        let mut nobody_model = gd_model_node.bind_mut();
        let model: llm::Model = nobody_model.get_model().map_err(|e| e.to_string())?;

        Ok(model)
    }

    #[func]
    /// Starts the embedding worker thread. This is called automatically when you call `embed`, if it wasn't already called.
    fn start_worker(&mut self) {
        let mut result = || -> Result<(), String> {
            let model = self.get_model()?;

            // TODO: configurable n_ctx
            self.embed_handle = Some(nobodywho::embed::EmbeddingsHandle::new(model, 4096));
            Ok(())
        };
        // run it and show error in godot if it fails
        if let Err(msg) = result() {
            godot_error!("Error running model: {}", msg);
        }
    }

    #[func]
    /// Generates the embedding of a text string. This will return a signal that you can use to wait for the embedding.
    /// The signal will return a PackedFloat32Array.
    fn embed(&mut self, text: String) -> Signal {
        if let Some(embed_handle) = &self.embed_handle {
            let mut embedding_channel = embed_handle.embed_text(text);
            let mut emit_node = self.to_gd();
            godot::task::spawn(async move {
                match embedding_channel.recv().await {
                    Some(embd) => emit_node
                        .signals()
                        .embedding_finished()
                        .emit(&PackedFloat32Array::from(embd)),
                    None => {
                        godot_error!("Failed generating embedding.");
                    }
                }
            });
        } else {
            godot_warn!("Worker was not started yet, starting now... You may want to call `start_worker()` ahead of time to avoid waiting.");
            self.start_worker();
            return self.embed(text);
        };

        // returns signal, so that you can `var vec = await embed("Hello, world!")`
        return godot::builtin::Signal::from_object_signal(&self.base_mut(), "embedding_finished");
    }

    #[func]
    /// Calculates the similarity between two embedding vectors.
    /// Returns a value between 0 and 1, where 1 is the highest similarity.
    fn cosine_similarity(a: PackedFloat32Array, b: PackedFloat32Array) -> f32 {
        nobodywho::embed::cosine_similarity(a.as_slice(), b.as_slice())
    }
}
