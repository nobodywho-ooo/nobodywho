use std::rc::Rc;
use std::sync::Arc;

use godot::prelude::*;

use crate::convert::{dict_get, json_to_variant, resolve_godot_path, variant_to_json};
use crate::model::NobodyWhoModel;
use crate::sampler::NobodyWhoSamplerConfig;
use crate::task::{on_blocking_thread, task};

/// A chat session over a loaded model. Cheap to share (internally `Arc`).
///
/// Build it with the async factory:
/// ```gdscript
/// var chat = await NobodyWhoChat.create(model, {})
/// ```
/// `model` is a `NobodyWhoModel` or a path String (loaded with default options).
/// Resolves to the chat, or null on failure (with a `godot_error!`).
///
/// Every async method returns an awaitable value. Await it immediately
/// (`var x = await chat.foo()`). Storing the return value and awaiting it
/// after another await/frame is unsupported and may hang.
#[derive(GodotClass)]
#[class(no_init, base=RefCounted)]
pub struct NobodyWhoChat {
    handle: nobodywho::chat::ChatHandleAsync,
    base: Base<RefCounted>,
}

#[godot_api]
impl NobodyWhoChat {
    /// Create a chat asynchronously. Worker init runs off the main thread.
    /// `await create(...)` resolves to a NobodyWhoChat, or null. Await it
    /// immediately.
    ///
    /// `model` is a NobodyWhoModel, or a path String (loaded with default
    /// model options, use_gpu=true).
    ///
    /// `config` is a Dictionary with optional keys:
    /// - `"system_prompt"` (String): the system prompt.
    /// - `"n_ctx"` (int): context window size (default 4096).
    /// - `"n_threads"` (int): inference thread count (default: auto-detect).
    /// - `"use_gpu"` (bool, default true): only used when `model` is a path;
    ///   a NobodyWhoModel already carries its own GPU setting.
    /// - `"sampler"` (NobodyWhoSamplerConfig): sampler chain. Default: the
    ///   core default (top_k=20, top_p=0.95, temperature=0.6, dist).
    /// - `"template_variables"` (Dictionary String->bool): chat-template vars.
    ///
    /// Pass `{}` for defaults. Unrecognized keys are ignored; a recognized
    /// key with a value of the wrong type is an error (resolves to null).
    #[func]
    fn create(model: Variant, config: VarDictionary) -> Variant {
        let (chat_config, use_gpu) = match Self::parse_config(&config) {
            Ok(parsed) => parsed,
            Err(e) => {
                godot_error!("NobodyWhoChat.create: {e}");
                return Variant::nil();
            }
        };
        task(async move {
            // Resolve model-or-path to a shared Arc<Model>. A path is loaded
            // with use_gpu from config (default true); for richer control,
            // load a NobodyWhoModel first and pass that.
            let arc = if let Ok(m) = model.try_to::<Gd<NobodyWhoModel>>() {
                m.bind().inner.clone()
            } else if let Ok(path) = model.try_to::<GString>() {
                let path = resolve_godot_path(&path);
                match nobodywho::llm::get_model_async(path, use_gpu, None, None, None).await {
                    Ok(m) => Arc::new(m),
                    Err(e) => {
                        godot_error!("Failed to load model: {}", nobodywho::render_miette(&e));
                        return Variant::nil();
                    }
                }
            } else {
                godot_error!("NobodyWhoChat.create() expects a NobodyWhoModel or a path String");
                return Variant::nil();
            };
            // ChatHandleAsync::new blocks on worker init (sync channel recv),
            // so run it off the main thread.
            let result =
                on_blocking_thread(move || nobodywho::chat::ChatHandleAsync::new(arc, chat_config))
                    .await;
            match result {
                Some(Ok(handle)) => {
                    Gd::from_init_fn(|base| NobodyWhoChat { handle, base }).to_variant()
                }
                Some(Err(e)) => {
                    godot_error!("Failed to create chat: {}", nobodywho::render_miette(&e));
                    Variant::nil()
                }
                None => {
                    godot_error!("Chat worker init panicked");
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Start generating a response. Returns immediately with a per-call token
    /// stream; pull tokens via `next_token()`, or await the full text via
    /// `completed()`.
    #[func]
    fn ask(&self, prompt: GString) -> Gd<NobodyWhoTokenStream> {
        NobodyWhoTokenStream::wrap(self.handle.ask(prompt.to_string()))
    }

    /// Stop the current generation early. Chat-scoped: with queued concurrent
    /// asks, stops whatever is currently generating.
    #[func]
    fn stop_generation(&self) {
        self.handle.stop_generation();
    }

    // --- Phase 2: query/mutation. Each returns the internal task's wait()
    // value directly (value-or-Signal); the caller awaits it immediately. ---

    /// Get the chat history as an Array of message dicts (`{role, content, ...}`).
    /// Resolves to the Array, or null on failure.
    #[func]
    fn get_chat_history(&self) -> Variant {
        let handle = self.handle.clone();
        task(async move {
            match handle.get_chat_history().await {
                Ok(msgs) => match serde_json::to_value(&msgs) {
                    Ok(json) => json_to_variant(&json),
                    Err(e) => {
                        godot_error!("get_chat_history: failed to serialize messages: {e}");
                        Variant::nil()
                    }
                },
                Err(e) => {
                    godot_error!("get_chat_history failed: {}", nobodywho::render_miette(&e));
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Replace the chat history. `messages` is an Array of message dicts
    /// (`{role, content, ...}`). Resolves to null (success) or null + error.
    #[func]
    fn set_chat_history(&self, messages: VarArray) -> Variant {
        let handle = self.handle.clone();
        let json = match variant_to_json(&messages.to_variant()) {
            Ok(v) => v,
            Err(e) => {
                godot_error!("set_chat_history: bad message shape: {e}");
                return Variant::nil();
            }
        };
        let msgs: Vec<nobodywho::chat::Message> = match serde_json::from_value(json) {
            Ok(m) => m,
            Err(e) => {
                godot_error!("set_chat_history: invalid messages: {e}");
                return Variant::nil();
            }
        };
        task(async move {
            match handle.set_chat_history(msgs).await {
                Ok(()) => Variant::nil(),
                Err(e) => {
                    godot_error!("set_chat_history failed: {}", nobodywho::render_miette(&e));
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Clear the conversation history, keeping the system prompt and tools.
    /// Resolves to null (success) or null + error.
    #[func]
    fn reset_history(&self) -> Variant {
        let handle = self.handle.clone();
        task(async move {
            match handle.reset_history().await {
                Ok(()) => Variant::nil(),
                Err(e) => {
                    godot_error!("reset_history failed: {}", nobodywho::render_miette(&e));
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Get the system prompt. Resolves to a String, or null if none is set
    /// (or on failure).
    #[func]
    fn get_system_prompt(&self) -> Variant {
        let handle = self.handle.clone();
        task(async move {
            match handle.get_system_prompt().await {
                Ok(Some(s)) => GString::from(&s).to_variant(),
                Ok(None) => Variant::nil(),
                Err(e) => {
                    godot_error!("get_system_prompt failed: {}", nobodywho::render_miette(&e));
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Update the system prompt without resetting history. Pass a String to
    /// set it, or null to clear it. Resolves to null (success) or null + error.
    #[func]
    fn set_system_prompt(&self, prompt: Variant) -> Variant {
        let handle = self.handle.clone();
        let prompt = if prompt.get_type() == godot::builtin::VariantType::NIL {
            None
        } else {
            match prompt.try_to::<GString>() {
                Ok(s) => Some(s.to_string()),
                Err(_) => {
                    godot_error!("set_system_prompt: expected String or null");
                    return Variant::nil();
                }
            }
        };
        task(async move {
            match handle.set_system_prompt(prompt).await {
                Ok(()) => Variant::nil(),
                Err(e) => {
                    godot_error!("set_system_prompt failed: {}", nobodywho::render_miette(&e));
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Get the current sampler config as a NobodyWhoSamplerConfig.
    /// Resolves to the config, or null on failure.
    #[func]
    fn get_sampler_config(&self) -> Variant {
        let handle = self.handle.clone();
        task(async move {
            match handle.get_sampler_config().await {
                Ok(cfg) => NobodyWhoSamplerConfig::wrap(cfg).to_variant(),
                Err(e) => {
                    godot_error!(
                        "get_sampler_config failed: {}",
                        nobodywho::render_miette(&e)
                    );
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Update the sampler config. `config` is a NobodyWhoSamplerConfig.
    /// Resolves to null (success) or null + error.
    #[func]
    fn set_sampler_config(&self, config: Gd<NobodyWhoSamplerConfig>) -> Variant {
        let handle = self.handle.clone();
        let cfg = config.bind().inner.clone();
        task(async move {
            match handle.set_sampler_config(cfg).await {
                Ok(()) => Variant::nil(),
                Err(e) => {
                    godot_error!(
                        "set_sampler_config failed: {}",
                        nobodywho::render_miette(&e)
                    );
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Get all chat-template variables as a Dictionary String->bool.
    /// Resolves to the Dictionary, or null on failure.
    #[func]
    fn get_template_variables(&self) -> Variant {
        let handle = self.handle.clone();
        task(async move {
            match handle.get_template_variables().await {
                Ok(vars) => {
                    let mut dict: VarDictionary = Dictionary::new();
                    for (k, v) in vars {
                        let _ = dict.insert(&GString::from(&k), &v.to_variant());
                    }
                    dict.to_variant()
                }
                Err(e) => {
                    godot_error!(
                        "get_template_variables failed: {}",
                        nobodywho::render_miette(&e)
                    );
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Set a single chat-template variable. Resolves to null (success) or
    /// null + error.
    #[func]
    fn set_template_variable(&self, name: GString, value: bool) -> Variant {
        let handle = self.handle.clone();
        let name = name.to_string();
        task(async move {
            match handle.set_template_variable(name, value).await {
                Ok(()) => Variant::nil(),
                Err(e) => {
                    godot_error!(
                        "set_template_variable failed: {}",
                        nobodywho::render_miette(&e)
                    );
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Replace all chat-template variables. `vars` is a Dictionary String->bool.
    /// Resolves to null (success) or null + error.
    #[func]
    fn set_template_variables(&self, vars: VarDictionary) -> Variant {
        let handle = self.handle.clone();
        let vars = match collect_template_variables(&vars) {
            Ok(v) => v,
            Err(e) => {
                godot_error!("set_template_variables: {e}");
                return Variant::nil();
            }
        };
        task(async move {
            match handle.set_template_variables(vars).await {
                Ok(()) => Variant::nil(),
                Err(e) => {
                    godot_error!(
                        "set_template_variables failed: {}",
                        nobodywho::render_miette(&e)
                    );
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Context usage statistics. Resolves to a Dictionary
    /// `{context_size: int, context_used: int}`, or null on failure.
    #[func]
    fn get_stats(&self) -> Variant {
        let handle = self.handle.clone();
        task(async move {
            match handle.get_stats().await {
                Ok(stats) => {
                    let mut dict: VarDictionary = Dictionary::new();
                    let _ = dict.insert(
                        &GString::from("context_size"),
                        &stats.context_size.to_variant(),
                    );
                    let _ = dict.insert(
                        &GString::from("context_used"),
                        &stats.context_used.to_variant(),
                    );
                    dict.to_variant()
                }
                Err(e) => {
                    godot_error!("get_stats failed: {}", nobodywho::render_miette(&e));
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// MTP draft acceptance rate for the most recent generation, in [0.0, 1.0].
    /// Resolves to a float, null if no drafts were proposed, or null on error.
    #[func]
    fn mtp_acceptance_rate(&self) -> Variant {
        let handle = self.handle.clone();
        task(async move {
            match handle.mtp_acceptance_rate().await {
                Ok(Some(rate)) => rate.to_variant(),
                Ok(None) => Variant::nil(),
                Err(e) => {
                    godot_error!(
                        "mtp_acceptance_rate failed: {}",
                        nobodywho::render_miette(&e)
                    );
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Tokenize a prompt. Resolves to an Array of ints (with null slots for
    /// media embedding positions), or null on failure.
    #[func]
    fn tokenize(&self, prompt: GString) -> Variant {
        let handle = self.handle.clone();
        let prompt = prompt.to_string();
        task(async move {
            match handle.tokenize(prompt).await {
                Ok(ids) => {
                    let mut arr: VarArray = Array::new();
                    for id in ids {
                        match id {
                            Some(i) => arr.push(&i.to_variant()),
                            None => arr.push(&Variant::nil()),
                        }
                    }
                    arr.to_variant()
                }
                Err(e) => {
                    godot_error!("tokenize failed: {}", nobodywho::render_miette(&e));
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }
}

impl NobodyWhoChat {
    /// Parse the `create` config Dictionary into a core `ChatConfig` plus the
    /// `use_gpu` flag (used only when the model is given as a path). Errors
    /// on any recognized key holding a value of the wrong type.
    fn parse_config(config: &VarDictionary) -> Result<(nobodywho::chat::ChatConfig, bool), String> {
        let defaults = nobodywho::chat::ChatConfig::default();
        let chat_config = nobodywho::chat::ChatConfig {
            system_prompt: dict_get::<GString>(config, "system_prompt")?
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty()),
            n_ctx: dict_get::<u32>(config, "n_ctx")?.unwrap_or(defaults.n_ctx),
            n_threads: dict_get::<u32>(config, "n_threads")?,
            sampler_config: dict_get::<Gd<NobodyWhoSamplerConfig>>(config, "sampler")?
                .map(|gd| gd.bind().inner.clone()),
            template_variables: dict_get::<VarDictionary>(config, "template_variables")?
                .map(|d| collect_template_variables(&d))
                .transpose()?
                .unwrap_or_default(),
            ..defaults
        };
        let use_gpu = dict_get::<bool>(config, "use_gpu")?.unwrap_or(true);
        Ok((chat_config, use_gpu))
    }
}

/// Parse a `Dictionary String->bool` of template variables. Errors on a
/// non-String key or a non-bool value.
fn collect_template_variables(
    dict: &VarDictionary,
) -> Result<std::collections::HashMap<String, bool>, String> {
    dict.iter_shared()
        .map(|(k, v)| {
            let key = k
                .try_to::<GString>()
                .map_err(|_| format!("template variable key {k} is not a String"))?;
            let value = v
                .try_to::<bool>()
                .map_err(|_| format!("template variable \"{key}\" is not a bool"))?;
            Ok((key.to_string(), value))
        })
        .collect()
}

// --- NobodyWhoTokenStream ---------------------------------------------------

/// A per-call token stream from `NobodyWhoChat.ask`. One object per call,
/// isolating concurrent generations and their errors.
///
/// A thin lazy wrapper around core's `TokenStreamAsync` (mirrors the Python
/// binding): the stream only advances when you pull.
/// ```gdscript
/// var stream = chat.ask("Tell me about Denmark.")
/// while true:
///     var tok = await stream.next_token()   # String; null when done
///     if tok == null: break
///     $Label.text += tok
/// var full = await stream.completed()       # full text
/// ```
///
/// One pull (`next_token()` or `completed()`) at a time. Unpulled tokens are
/// buffered by core's channel, so generation never stalls on a slow consumer.
#[derive(GodotClass)]
#[class(no_init, base=RefCounted)]
pub struct NobodyWhoTokenStream {
    /// The core stream. Shared with in-flight pull tasks: a pull future needs
    /// `&mut` across an await, and neither a `bind()` nor a `RefCell` guard
    /// may be held across a suspension — so pulls clone the `Rc` and
    /// `lock().await` instead.
    stream: Rc<tokio::sync::Mutex<nobodywho::chat::TokenStreamAsync>>,
    base: Base<RefCounted>,
}

#[godot_api]
impl NobodyWhoTokenStream {
    /// Pull the next token. Resolves to a String, or null once the stream
    /// ends (exhausted, or failed — failures are logged with godot_error).
    /// Await the returned value immediately; one pull at a time.
    #[func]
    fn next_token(&self) -> Variant {
        let stream = self.stream.clone();
        task(async move {
            match stream.lock().await.next_token().await {
                Ok(Some(tok)) => GString::from(&tok).to_variant(),
                Ok(None) => Variant::nil(),
                Err(e) => {
                    godot_error!("Generation failed: {}", nobodywho::render_miette(&e));
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Await the full response text, draining the rest of the stream.
    /// Resolves to the full text (repeat calls included — core latches it),
    /// or null if this call observes a generation failure.
    #[func]
    fn completed(&self) -> Variant {
        let stream = self.stream.clone();
        task(async move {
            match stream.lock().await.completed().await {
                Ok(full) => GString::from(&full).to_variant(),
                Err(e) => {
                    godot_error!("Generation failed: {}", nobodywho::render_miette(&e));
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    // --- Throwaway Phase-1 smoke test ---------------------------------------
    // Wraps a synthetic core stream fed from a thread with delays, so both
    // pull paths get exercised: the inline fast path (token already queued)
    // and the suspend path (channel empty). No model needed. Removed once the
    // real ask() path is validated end-to-end in CI.
    #[func]
    fn _test_stream(tokens: Array<GString>) -> Gd<NobodyWhoTokenStream> {
        use nobodywho::stream::StreamOutput;
        let toks: Vec<String> = tokens.iter_shared().map(|g| g.to_string()).collect();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<
            StreamOutput<nobodywho::errors::CompletionError>,
        >();
        std::thread::spawn(move || {
            let full = toks.concat();
            for t in toks {
                std::thread::sleep(std::time::Duration::from_millis(10));
                let _ = tx.send(StreamOutput::Token(t));
            }
            let _ = tx.send(StreamOutput::Done(full));
        });
        Self::wrap(nobodywho::chat::TokenStreamAsync::new(rx))
    }
}

impl NobodyWhoTokenStream {
    /// Wrap a core token stream.
    fn wrap(stream: nobodywho::chat::TokenStreamAsync) -> Gd<Self> {
        Gd::from_init_fn(|base| Self {
            stream: Rc::new(tokio::sync::Mutex::new(stream)),
            base,
        })
    }
}
