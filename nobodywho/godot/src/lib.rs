mod sampler_resource;

use godot::classes::{INode, ProjectSettings};
use godot::prelude::*;
use nobodywho::{chat_state, llm, sampler_config};
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, error, info, trace, warn};
use tracing_subscriber::prelude::*;

use crate::sampler_resource::NobodyWhoSampler;

struct NobodyWhoExtension;

#[gdextension]
unsafe impl ExtensionLibrary for NobodyWhoExtension {
    fn on_level_init(level: InitLevel) {
        // this version logging needs to happen after godot has loaded
        // otherwise the tracing_subscriber stuff will crash, because it can't access godot stuff
        if level == InitLevel::Editor {
            // Initialize tracing_subscriber - sends all tracing events to godot console.
            set_log_level("INFO");
            info!("NobodyWho Godot version: {}", env!("CARGO_PKG_VERSION"));
        }
    }
}

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

#[godot_api]
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

    #[func]
    /// Sets the (global) log level of NobodyWho.
    /// Valid arguments are "TRACE", "DEBUG", "INFO", "WARN", and "ERROR".
    fn set_log_level(level: String) {
        set_log_level(&level);
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

    // internal state
    chat_handle: Option<nobodywho::chat::ChatHandle>,
    tools: Vec<nobodywho::chat::Tool>,
    signal_counter: AtomicU64,
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
            signal_counter: AtomicU64::new(0),
            tools: vec![],

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
                self.tools.clone(),
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
    fn stop_generation(&mut self) {
        if let Some(chat_handle) = &self.chat_handle {
            chat_handle.stop_generation();
        } else {
            godot_warn!("Attempted to stop generation, but no worker is running. Doing nothing.");
        }
    }

    #[func]
    fn reset_context(&mut self) {
        if let Some(chat_handle) = &self.chat_handle {
            chat_handle.reset_chat(self.system_prompt.to_string(), self.tools.clone());
        } else {
            godot_error!("Attempted to reset context, but no worker is running. Doing nothing.");
        }
    }

    #[func]
    fn get_chat_history(&mut self) -> Variant {
        if let Some(chat_handle) = &self.chat_handle {
            // kick off operation
            let mut rx = chat_handle.get_chat_history();

            // decide on a unique name for the response signal
            let signal_name = format!(
                "get_chat_history_{}",
                self.signal_counter.fetch_add(1, Ordering::Relaxed)
            );
            self.base_mut().add_user_signal(&signal_name);

            let mut emit_node = self.to_gd();
            let signal_name_copy = signal_name.clone();
            godot::task::spawn(async move {
                let Some(chat_history) = rx.recv().await else {
                    error!("Chat worker died while waiting for get_chat_history.");
                    emit_node.emit_signal(&signal_name_copy, &vec![]);
                    return;
                };
                let godot_dict_msgs: Array<Dictionary> = messages_to_dictionaries(&chat_history);
                let godot_variant_array: Array<Variant> = godot_dict_msgs
                    .iter_shared()
                    .map(|dict| Variant::from(dict))
                    .collect();

                // wait for godot code to connect to signal
                let signal = Signal::from_object_signal(&emit_node, &signal_name_copy);
                let mut tree: Gd<SceneTree> = godot::classes::Engine::singleton()
                    .get_main_loop()
                    .unwrap()
                    .cast();
                for _ in 0..10 {
                    if signal.connections().len() > 0 {
                        // happy path: signal has a connection.
                        signal.emit(&vec![Variant::from(godot_variant_array)]);
                        // we're done.
                        return;
                    };
                    // wait one frame before checking number of connections again
                    trace!("Nothing connected to signal yet, waiting one frame...");
                    tree.signals().process_frame().to_future().await;
                }
                // unhappy path: nothing ever connected:
                warn!("Nothing connected to get_chat_history signal for 10 frames. Giving up...");
            });

            // returns signal, so that you can `var msgs = await get_chat_history()`
            Variant::from(godot::builtin::Signal::from_object_signal(
                &self.base_mut(),
                &signal_name,
            ))
        } else {
            godot_error!("Attempted to reset context, but no worker is running. Doing nothing and returning nil.");
            Variant::nil()
        }
    }

    #[func]
    fn set_chat_history(&mut self, messages: Array<Variant>) {
        if let Some(chat_handle) = &self.chat_handle {
            match dictionaries_to_messages(messages) {
                Ok(msg_vec) => {
                    // Check if last message is from user and warn
                    if msg_vec
                        .last()
                        .map_or(false, |msg| msg.role() == &chat_state::Role::User)
                    {
                        godot_warn!("Chat history ends with a user message. This may cause unexpected behavior during generation.");
                    }

                    let _rx = chat_handle.set_chat_history(msg_vec);
                    // we ignore the receiver for now, fire-and-forget
                }
                Err(e) => godot_error!("Failed to set chat history: {}", e),
            }
        } else {
            godot_error!("Attempted to set chat history, but no worker is running. Doing nothing.");
        }
    }

    #[func]
    /// Add a tool for the LLM to use.
    /// Tool calling is only supported for a select few models. We recommend Qwen3.
    ///
    /// The tool is a fully typed callable function on a godot object.
    /// The function should return a string.
    /// All parameters should have type hints, and only primitive types are supported.
    /// NobodyWho will use the type hints to constrain the generation, such that the function will
    /// only ever be called with the correct types.
    /// Fancier types like lists, dictionaries, and classes are not (yet) supported.
    ///
    /// If you need to specify more parameter constraints, see `add_tool_with_schema`.
    ///
    /// Example:
    ///
    /// ```
    /// extends NobodyWhoChat
    ///
    /// func add_numbers(a: int, b: int):
    ///     return str(a + b)
    ///
    /// func _ready():
    ///     # register the tool
    ///     add_tool(add_numbers, "Adds two integers")
    ///
    ///     # see that the llm invokes the tool
    ///     say("What is two plus two?")
    /// ```
    fn add_tool(&mut self, callable: Callable, description: String) {
        if self.chat_handle.is_some() {
            godot_warn!("Worker already running. Tools won't be available until restart or reset");
        }

        let json_schema = match json_schema_from_callable(&callable) {
            Ok(js) => js,
            Err(e) => {
                godot_error!("Failed generating json schema for function: {e}");
                return;
            }
        };
        debug!(?json_schema);

        return self._add_tool_with_schema(callable, description, json_schema);
    }

    #[func]
    /// Add a tool for the LLM to use, along with a json schema to constrain the parameters.
    /// The order of parameters in the json schema must be preserved.
    /// The json schema keyword "description" may be used here, to help guide the LLM.
    /// Tool calling is only supported for a select few models. We recommend Qwen3.
    ///
    /// Example:
    ///
    /// ```
    /// extends NobodyWhoChat
    ///
    /// func add_numbers(a, b):
    ///     return str(a + b)
    ///
    /// func _ready():
    ///     # register the tool
    ///     var json_schema = """
    ///         {
    ///           "type": "object",
    ///           "properties": {
    ///             "a": { "type": "integer" },
    ///             "b": { "type": "integer" }
    ///           },
    ///           "required": ["a", "b"],
    ///         }
    ///     """
    ///     add_tool_with_schema(add_numbers, "Adds two integers", json_schema)
    ///
    ///     # see that the llm invokes the tool
    ///     say("What is two plus two?")
    /// ```
    fn add_tool_with_schema(
        &mut self,
        callable: Callable,
        description: String,
        json_schema: String,
    ) {
        let Ok(serde_json::Value::Object(json_schema)) = serde_json::from_str(json_schema.as_str())
        else {
            godot_error!("Passed json schema was not a valid json object.");
            return;
        };
        return self._add_tool_with_schema(callable, description, json_schema);
    }

    fn _add_tool_with_schema(
        &mut self,
        callable: Callable,
        description: String,
        json_schema: serde_json::Map<String, serde_json::Value>,
    ) {
        // list of property names, preserving order of arguments from Callable
        let Some(properties) = json_schema
            .get("properties")
            .and_then(|v| v.as_object())
            .map(|obj| obj.keys().cloned().collect::<Vec<String>>())
        else {
            godot_error!("JSON Schema was malformed");
            return;
        };

        let Some(method_name) = callable.method_name() else {
            godot_error!("Could not get method name. Did you pass an anonymous function?");
            return;
        };

        // the callback that the actual tool call uses
        let func = move |j: serde_json::Value| {
            let Some(obj) = j.as_object() else {
                warn!("LLM passed bad arguments to tool: {j:?}");
                return "Error: Bad arguments. You must supply a json object.".into();
            };

            let mut args: Vec<Variant> = vec![];
            for prop in &properties {
                let Some(val) = obj.get(prop.as_str()) else {
                    warn!("LLM passed bad arguments to tool. Missing argument {prop}");
                    return format!("Error: Missing argument {prop}");
                };
                args.push(json_to_godot(val));
            }

            // TODO: if arguments are incorrect here, the callable returns null
            let res = callable.call(&args);
            res.to_string()
        };
        let new_tool = nobodywho::chat::Tool::new(
            method_name.into(),
            description,
            json_schema.into(),
            std::sync::Arc::new(func),
        );
        self.tools.push(new_tool);
    }

    #[signal]
    /// Triggered when a new token is received from the LLM. Returns the new token as a string.
    /// It is strongly recommended to connect to this signal, and display the text output as it is
    /// being generated. This makes for a much nicer user experience.
    fn response_updated(new_token: GString);

    #[signal]
    /// Triggered when the LLM has finished generating the response. Returns the full response as a string.
    fn response_finished(response: GString);

    #[func]
    /// Sets the (global) log level of NobodyWho.
    /// Valid arguments are "TRACE", "DEBUG", "INFO", "WARN", and "ERROR".
    fn set_log_level(level: String) {
        set_log_level(&level);
    }
}

fn json_to_godot(value: &serde_json::Value) -> Variant {
    match value {
        serde_json::Value::Null => Variant::nil(),
        serde_json::Value::Bool(b) => Variant::from(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Variant::from(i)
            } else if let Some(u) = n.as_u64() {
                Variant::from(u)
            } else if let Some(f) = n.as_f64() {
                Variant::from(f)
            } else {
                warn!("Didn't expect this code branch to be possible. Trying fallible conversion to f64.");
                Variant::from(n.as_f64().unwrap())
            }
        }
        serde_json::Value::String(s) => Variant::from(s.as_str()),
        serde_json::Value::Array(arr) => {
            let vec: Vec<Variant> = arr.into_iter().map(json_to_godot).collect();
            Variant::from(vec)
        }
        serde_json::Value::Object(obj) => {
            // XXX: this is prerty lazy
            let mut dict = Dictionary::new();
            for (key, val) in obj {
                dict.set(key.as_str(), json_to_godot(val));
            }
            Variant::from(dict)
        }
    }
}

fn json_schema_from_callable(
    callable: &Callable,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    // find method metadata
    let method_name = callable.method_name().ok_or("Error adding tool: Could not get method name for callable. Did you pass in an anonymous function?".to_string())?;
    let method_obj = callable.object().ok_or("Could not find object for callable. Anonymous functions and static methods are not supported.".to_string())?;
    let method_info = method_obj
        .get_method_list()
        .iter_shared()
        // XXX: I expect that this bit is pretty slow. But it works for now...
        .find(|dict| dict.at("name").to::<String>() == method_name.to_string());
    let method_info = method_info.ok_or("Could not find method on this object. Is the method you passed defined on the NobodyWhoChat script?".to_string())?;
    let method_args: Array<Dictionary> = method_info.at("args").to();

    // start building json schema
    let mut properties = serde_json::Map::new();
    let mut required = vec![];

    for arg in method_args.iter_shared() {
        let arg_name: String = arg.at("name").to();
        let arg_type: VariantType = arg.at("type").to();
        let arg_type_json_schema_name: &str = match arg_type {
            VariantType::NIL => return Err(format!("Error adding tool {method_name}: arguments must all have type hints. Argument '{arg_name}' does not have a type hint.")),
            VariantType::BOOL => "boolean",
            VariantType::INT => "integer",
            VariantType::FLOAT => "number",
            VariantType::STRING => "string",
            VariantType::ARRAY => "array",
            // TODO: more types. E.g. Object, Vector types, Array types, Dictionary
            _ => {
                return Err(format!("Error adding tool {method_name} - Unsupported type for argument '{arg_name}': {arg_type:?}"));
            }
        };

        properties.insert(
            arg_name.clone(),
            serde_json::json!({ "type": arg_type_json_schema_name }),
        );
        // TODO: can we make arguments with default values not required?
        required.push(serde_json::Value::String(arg_name));
    }

    let mut result = serde_json::Map::new();
    result.insert("type".into(), "object".into());
    result.insert("properties".into(), properties.into());
    result.insert("required".into(), required.into());
    Ok(result)
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

    #[func]
    /// Sets the (global) log level of NobodyWho.
    /// Valid arguments are "TRACE", "DEBUG", "INFO", "WARN", and "ERROR".
    fn set_log_level(level: String) {
        set_log_level(&level);
    }
}

#[derive(GodotClass)]
#[class(base=Node)]
/// The Rerank node is used to rank documents based on their relevance to a query.
/// This is useful for document retrieval and information retrieval tasks.
///
/// It requires a "NobodyWhoModel" node to be set with a GGUF model capable of reranking.
/// Example:
///
/// ```
/// extends NobodyWhoRerank
///
/// func _ready():
///     # configure node
///     self.model_node = get_node("../RerankModel")
///
///     # rank documents
///     var query = "What is the capital of France?"
///     var documents = PackedStringArray([
///         "Paris is the capital of France.",
///         "France is a country in Europe.",
///         "The Eiffel Tower is in Paris."
///     ])
///     var ranked_docs = await rank(query, documents, 2)
///     print("Top 2 documents: " + str(ranked_docs))
/// ```
///
struct NobodyWhoRerank {
    #[export]
    /// The model node for the reranker.
    model_node: Option<Gd<NobodyWhoModel>>,
    rerank_handle: Option<nobodywho::rerank::RerankerHandle>,
    base: Base<Node>,
}

#[godot_api]
impl INode for NobodyWhoRerank {
    fn init(base: Base<Node>) -> Self {
        Self {
            model_node: None,
            rerank_handle: None,
            base,
        }
    }
}

#[godot_api]
impl NobodyWhoRerank {
    #[signal]
    /// Triggered when the ranking has finished. Returns the ranked documents as a PackedStringArray.
    fn ranking_finished(ranked_documents: PackedStringArray);

    fn get_model(&mut self) -> Result<llm::Model, String> {
        let gd_model_node = self.model_node.as_mut().ok_or("Model node was not set")?;
        let mut nobody_model = gd_model_node.bind_mut();
        let model: llm::Model = nobody_model.get_model().map_err(|e| e.to_string())?;

        Ok(model)
    }

    #[func]
    /// Starts the reranker worker thread. This is called automatically when you call `rank`, if it wasn't already called.
    fn start_worker(&mut self) {
        let mut result = || -> Result<(), String> {
            let model = self.get_model()?;

            // TODO: configurable n_ctx liek with the embeddings node
            self.rerank_handle = Some(nobodywho::rerank::RerankerHandle::new(model, 4096));
            Ok(())
        };
        
        if let Err(msg) = result() {
            godot_error!("Error running model: {}", msg);
        }
    }

    #[func]
    /// Ranks documents based on their relevance to the query.
    /// Returns a signal that you can use to wait for the ranking.
    /// The signal will return a PackedStringArray of ranked documents.
    /// 
    /// Parameters:
    /// - query: The question or query to rank documents against
    /// - documents: Array of document strings to rank
    /// - limit: Maximum number of documents to return (-1 for all documents)
    fn rank(&mut self, query: String, documents: PackedStringArray, limit: i32) -> Signal {
        if let Some(rerank_handle) = &self.rerank_handle {
            let documents: Vec<String> = documents
                .to_vec()
                .into_iter()
                .map(|s| s.to_string())
                .collect();
            
            let mut ranking_channel = rerank_handle.rank(query, documents.clone());
            let mut emit_node = self.to_gd();
            godot::task::spawn(async move {
                match ranking_channel.recv().await {
                    Some(scores) => {
                        // Create pairs of (document, score) and sort by score
                        let mut docs_with_scores: Vec<(String, f32)> = documents
                            .into_iter()
                            .zip(scores.into_iter())
                            .collect();

                        // Sort by score (highest score first)
                        docs_with_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                        
                        // Extract just the documents, optionally limiting
                        let ranked_docs: Vec<String> = if limit > 0 {
                            docs_with_scores
                                .into_iter()
                                .take(limit as usize)
                                .map(|(doc, _)| doc)
                                .collect()
                        } else {
                            docs_with_scores
                                .into_iter()
                                .map(|(doc, _)| doc)
                                .collect()
                        };
                        
                        let gstring_array: Vec<GString> = ranked_docs.into_iter().map(|s| GString::from(s)).collect();

                        let result = PackedStringArray::from(gstring_array);
                        emit_node
                            .signals()
                            .ranking_finished()
                            .emit(&result);
                    }
                    None => {
                        godot_error!("Failed generating ranking.");
                    }
                }
            });
        } else {
            godot_warn!("Worker was not started yet, starting now... You may want to call `start_worker()` ahead of time to avoid waiting.");
            self.start_worker();
            return self.rank(query, documents, limit);
        };

        // returns signal, so that you can `var ranked = await rank("query", docs, 5)`
        return godot::builtin::Signal::from_object_signal(&self.base_mut(), "ranking_finished");
    }

    #[func]
    /// Sets the (global) log level of NobodyWho.
    /// Valid arguments are "TRACE", "DEBUG", "INFO", "WARN", and "ERROR".
    fn set_log_level(level: String) {
        set_log_level(&level);
    }
}

/// Small utility to convert our internal Messsage type to godot dictionaries.
fn messages_to_dictionaries(messages: &[chat_state::Message]) -> Array<Dictionary> {
    messages
        .iter()
        .map(|msg| {
            let json_value = serde_json::to_value(msg).unwrap_or_default();
            if let serde_json::Value::Object(obj) = json_value {
                obj.into_iter()
                    .map(|(k, v)| {
                        let variant = match v {
                            serde_json::Value::String(s) => Variant::from(s),
                            serde_json::Value::Array(arr) => {
                                // Convert arrays (like tool_calls) to proper Godot format
                                let godot_array: Array<Variant> = arr
                                    .into_iter()
                                    .map(|item| match item {
                                        serde_json::Value::Object(obj) => {
                                            let mut dict = Dictionary::new();
                                            for (key, val) in obj {
                                                dict.set(key, json_to_godot(&val));
                                            }
                                            Variant::from(dict)
                                        }
                                        _ => json_to_godot(&item),
                                    })
                                    .collect();
                                Variant::from(godot_array)
                            }
                            _ => json_to_godot(&v),
                        };
                        (GString::from(k), variant)
                    })
                    .collect()
            } else {
                Dictionary::new()
            }
        })
        .collect()
}

/// Small utility to convert godot dictionaries back to our internal Message type.
fn dictionaries_to_messages(dicts: Array<Variant>) -> Result<Vec<chat_state::Message>, String> {
    dicts
        .iter_shared()
        .map(|variant| {
            // First convert the Variant to Dictionary
            let dict = variant
                .try_to::<Dictionary>()
                .map_err(|_| "Array element is not a Dictionary")?;

            // Convert Dictionary to serde_json::Value
            let mut json_obj = serde_json::Map::new();
            for (key, value) in dict.iter_shared() {
                let key_str = key
                    .try_to::<GString>()
                    .map_err(|_| "Dictionary key is not a string")?
                    .to_string();
                let value_str = value
                    .try_to::<GString>()
                    .map_err(|_| "Dictionary value is not a string")?
                    .to_string();
                json_obj.insert(key_str, serde_json::Value::String(value_str));
            }

            // Deserialize using serde
            serde_json::from_value(serde_json::Value::Object(json_obj))
                .map_err(|e| format!("Failed to deserialize message: {}", e))
        })
        .collect()
}

// LOGGING

// Writer that forwards to Godot logging
struct GodotWriter;

impl std::io::Write for GodotWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(s) = std::str::from_utf8(buf) {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                // Check if it's an error message (simplistic approach)
                // You might want more sophisticated detection based on your format
                if trimmed.contains("ERROR") {
                    godot_error!("{}", trimmed);
                } else if trimmed.contains("WARN") {
                    godot_warn!("{}", trimmed);
                } else {
                    godot_print!("{}", trimmed);
                }
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for GodotWriter {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        GodotWriter
    }
}

static INIT: std::sync::Once = std::sync::Once::new();
// Static handle to the filter
static LEVEL_HANDLE: std::sync::Mutex<
    Option<
        tracing_subscriber::reload::Handle<
            tracing_subscriber::filter::LevelFilter,
            tracing_subscriber::Registry,
        >,
    >,
> = std::sync::Mutex::new(None);

pub fn set_log_level(level_str: &str) {
    let level: tracing::Level = match level_str.to_uppercase().parse() {
        Ok(level) => level,
        Err(e) => {
            godot_error!("Invalid log level '{level_str}': {e}");
            return;
        }
    };

    // First-time initialization
    INIT.call_once(|| {
        nobodywho::send_llamacpp_logs_to_tracing();

        // Create a reloadable filter
        let (filter, filter_handle) = tracing_subscriber::reload::Layer::new(
            tracing_subscriber::filter::LevelFilter::from_level(level),
        );
        // Store the handle for future updates
        *LEVEL_HANDLE.lock().unwrap() = Some(filter_handle);

        // Let fmt layer handle the formatting, but use our custom writer
        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_writer(GodotWriter)
            .with_ansi(false)
            .with_level(true)
            .compact();

        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .init();
    });

    if let Some(handle) = &*LEVEL_HANDLE.lock().unwrap() {
        if let Err(e) = handle.modify(|filter| {
            *filter = tracing_subscriber::filter::LevelFilter::from_level(level);
        }) {
            godot_error!("Failed to update log level: {}", e);
        }
    }
}
