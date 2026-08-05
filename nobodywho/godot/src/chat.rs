use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use godot::prelude::*;

use crate::convert::{dict_get, json_to_variant, resolve_godot_path, variant_to_json};
use crate::model::NobodyWhoModel;
use crate::sampler::NobodyWhoSamplerConfig;
use crate::task::{on_blocking_thread, task};
use crate::tools::NobodyWhoTool;

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
    /// Set while one of this chat's tools is running (the worker is blocked
    /// waiting for it). Chat methods check it and fail fast instead of
    /// hanging — see `guarded()` and TOOLS_DESIGN.md §3.
    reentrancy_flag: Arc<AtomicBool>,
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
    /// - `"tools"` (Array of NobodyWhoTool): tools the model can call.
    ///
    /// Pass `{}` for defaults. Unrecognized keys are ignored; a recognized
    /// key with a value of the wrong type is an error (resolves to null).
    #[func]
    fn create(model: Variant, config: VarDictionary) -> Variant {
        let reentrancy_flag = Arc::new(AtomicBool::new(false));
        let (chat_config, use_gpu) = match Self::parse_config(&config, &reentrancy_flag) {
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
                Some(Ok(handle)) => Gd::from_init_fn(|base| NobodyWhoChat {
                    handle,
                    reentrancy_flag,
                    base,
                })
                .to_variant(),
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
    ///
    /// Calling this from inside one of this chat's own tools queues the ask
    /// behind the current generation — but do **not** await its stream from
    /// inside the tool (the worker won't reach it until the tool returns).
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

    /// Replace the chat's tools. `tools` is an Array of `NobodyWhoTool`.
    /// Resolves to null on success, or null + `godot_error!` on failure.
    /// Each GDScript tool spawns its own main-thread loop; the previous
    /// tools' loops end themselves when core drops the old closures.
    #[func]
    fn set_tools(&self, tools: VarArray) -> Variant {
        let core_tools = build_core_tools(&tools, &self.reentrancy_flag);
        let handle = self.handle.clone();
        self.guarded("set_tools", async move {
            handle
                .set_tools(core_tools)
                .await
                .map(|()| Variant::nil())
                .map_err(|e| e.to_string())
        })
    }

    /// Reset the chat with a new system prompt and tool set. `system_prompt`
    /// is a String or null (clear). `tools` is an Array of `NobodyWhoTool`.
    /// Resolves to null on success, or null + `godot_error!` on failure.
    #[func]
    fn reset_chat(&self, system_prompt: Variant, tools: VarArray) -> Variant {
        let prompt = match opt_string(&system_prompt) {
            Ok(p) => p,
            Err(_) => {
                godot_error!("reset_chat: system_prompt must be a String or null");
                return Variant::nil();
            }
        };
        let core_tools = build_core_tools(&tools, &self.reentrancy_flag);
        let handle = self.handle.clone();
        self.guarded("reset_chat", async move {
            handle
                .reset_chat(prompt, core_tools)
                .await
                .map(|()| Variant::nil())
                .map_err(|e| e.to_string())
        })
    }

    /// Get the chat history as an Array of message dicts (`{role, content, ...}`).
    /// Resolves to the Array, or null on failure.
    #[func]
    fn get_chat_history(&self) -> Variant {
        let handle = self.handle.clone();
        self.guarded("get_chat_history", async move {
            let msgs = handle
                .get_chat_history()
                .await
                .map_err(|e| nobodywho::render_miette(&e))?;
            let json = serde_json::to_value(&msgs)
                .map_err(|e| format!("failed to serialize messages: {e}"))?;
            Ok(json_to_variant(&json))
        })
    }

    /// Replace the chat history. `messages` is an Array of message dicts
    /// (`{role, content, ...}`). Resolves to null (success) or null + error.
    #[func]
    fn set_chat_history(&self, messages: VarArray) -> Variant {
        let handle = self.handle.clone();
        let json = variant_to_json(&messages.to_variant());
        self.guarded("set_chat_history", async move {
            let msgs: Vec<nobodywho::chat::Message> =
                serde_json::from_value(json?).map_err(|e| format!("invalid messages: {e}"))?;
            handle
                .set_chat_history(msgs)
                .await
                .map(|()| Variant::nil())
                .map_err(|e| nobodywho::render_miette(&e))
        })
    }

    /// Clear the conversation history, keeping the system prompt and tools.
    /// Resolves to null (success) or null + error.
    #[func]
    fn reset_history(&self) -> Variant {
        let handle = self.handle.clone();
        self.guarded("reset_history", async move {
            handle
                .reset_history()
                .await
                .map(|()| Variant::nil())
                .map_err(|e| nobodywho::render_miette(&e))
        })
    }

    /// Get the system prompt. Resolves to a String, or null if none is set
    /// (or on failure).
    #[func]
    fn get_system_prompt(&self) -> Variant {
        let handle = self.handle.clone();
        self.guarded("get_system_prompt", async move {
            let prompt = handle
                .get_system_prompt()
                .await
                .map_err(|e| nobodywho::render_miette(&e))?;
            Ok(prompt.map_or(Variant::nil(), |s| GString::from(&s).to_variant()))
        })
    }

    /// Update the system prompt without resetting history. Pass a String to
    /// set it, or null to clear it. Resolves to null (success) or null + error.
    #[func]
    fn set_system_prompt(&self, prompt: Variant) -> Variant {
        let prompt = match opt_string(&prompt) {
            Ok(p) => p,
            Err(_) => {
                godot_error!("set_system_prompt: expected String or null");
                return Variant::nil();
            }
        };
        let handle = self.handle.clone();
        self.guarded("set_system_prompt", async move {
            handle
                .set_system_prompt(prompt)
                .await
                .map(|()| Variant::nil())
                .map_err(|e| nobodywho::render_miette(&e))
        })
    }

    /// Get the current sampler config as a NobodyWhoSamplerConfig.
    /// Resolves to the config, or null on failure.
    #[func]
    fn get_sampler_config(&self) -> Variant {
        let handle = self.handle.clone();
        self.guarded("get_sampler_config", async move {
            let cfg = handle
                .get_sampler_config()
                .await
                .map_err(|e| nobodywho::render_miette(&e))?;
            Ok(NobodyWhoSamplerConfig::wrap(cfg).to_variant())
        })
    }

    /// Update the sampler config. `config` is a NobodyWhoSamplerConfig.
    /// Resolves to null (success) or null + error.
    #[func]
    fn set_sampler_config(&self, config: Gd<NobodyWhoSamplerConfig>) -> Variant {
        let cfg = config.bind().inner.clone();
        let handle = self.handle.clone();
        self.guarded("set_sampler_config", async move {
            handle
                .set_sampler_config(cfg)
                .await
                .map(|()| Variant::nil())
                .map_err(|e| nobodywho::render_miette(&e))
        })
    }

    /// Get all chat-template variables as a Dictionary String->bool.
    /// Resolves to the Dictionary, or null on failure.
    #[func]
    fn get_template_variables(&self) -> Variant {
        let handle = self.handle.clone();
        self.guarded("get_template_variables", async move {
            let vars = handle
                .get_template_variables()
                .await
                .map_err(|e| nobodywho::render_miette(&e))?;
            let mut dict: VarDictionary = Dictionary::new();
            for (k, v) in vars {
                let _ = dict.insert(&GString::from(&k), &v.to_variant());
            }
            Ok(dict.to_variant())
        })
    }

    /// Set a single chat-template variable. Resolves to null (success) or
    /// null + error.
    #[func]
    fn set_template_variable(&self, name: GString, value: bool) -> Variant {
        let name = name.to_string();
        let handle = self.handle.clone();
        self.guarded("set_template_variable", async move {
            handle
                .set_template_variable(name, value)
                .await
                .map(|()| Variant::nil())
                .map_err(|e| nobodywho::render_miette(&e))
        })
    }

    /// Replace all chat-template variables. `vars` is a Dictionary String->bool.
    /// Resolves to null (success) or null + error.
    #[func]
    fn set_template_variables(&self, vars: VarDictionary) -> Variant {
        let vars = collect_template_variables(&vars);
        let handle = self.handle.clone();
        self.guarded("set_template_variables", async move {
            handle
                .set_template_variables(vars?)
                .await
                .map(|()| Variant::nil())
                .map_err(|e| nobodywho::render_miette(&e))
        })
    }

    /// Context usage statistics. Resolves to a Dictionary
    /// `{context_size: int, context_used: int}`, or null on failure.
    #[func]
    fn get_stats(&self) -> Variant {
        let handle = self.handle.clone();
        self.guarded("get_stats", async move {
            let stats = handle
                .get_stats()
                .await
                .map_err(|e| nobodywho::render_miette(&e))?;
            let mut dict: VarDictionary = Dictionary::new();
            let _ = dict.insert(
                &GString::from("context_size"),
                &stats.context_size.to_variant(),
            );
            let _ = dict.insert(
                &GString::from("context_used"),
                &stats.context_used.to_variant(),
            );
            Ok(dict.to_variant())
        })
    }

    /// MTP draft acceptance rate for the most recent generation, in [0.0, 1.0].
    /// Resolves to a float, null if no drafts were proposed, or null on error.
    #[func]
    fn mtp_acceptance_rate(&self) -> Variant {
        let handle = self.handle.clone();
        self.guarded("mtp_acceptance_rate", async move {
            let rate = handle
                .mtp_acceptance_rate()
                .await
                .map_err(|e| nobodywho::render_miette(&e))?;
            Ok(rate.map_or(Variant::nil(), |r| r.to_variant()))
        })
    }

    /// Tokenize a prompt. Resolves to an Array of ints (with null slots for
    /// media embedding positions), or null on failure.
    #[func]
    fn tokenize(&self, prompt: GString) -> Variant {
        let prompt = prompt.to_string();
        let handle = self.handle.clone();
        self.guarded("tokenize", async move {
            let ids = handle
                .tokenize(prompt)
                .await
                .map_err(|e| nobodywho::render_miette(&e))?;
            let arr: VarArray = ids
                .iter()
                .map(|id| id.map_or(Variant::nil(), |i| i.to_variant()))
                .collect();
            Ok(arr.to_variant())
        })
    }
}

impl NobodyWhoChat {
    /// The one shape every async chat method shares: check the re-entrancy
    /// guard, run the op as a latched task, and resolve errors to null +
    /// `godot_error!`. Returns the task's value-or-Signal Variant (await it
    /// immediately).
    fn guarded<Fut>(&self, op: &'static str, fut: Fut) -> Variant
    where
        Fut: std::future::Future<Output = Result<Variant, String>> + 'static,
    {
        if self.reentrancy_flag.load(Ordering::Acquire) {
            godot_error!(
                "{op}: called back into this chat from one of its own tools — the worker is \
                 blocked waiting for the tool to return, so this call can never complete. \
                 Use a different chat, or return from the tool first."
            );
            return Variant::nil();
        }
        task(async move {
            fut.await.unwrap_or_else(|e| {
                godot_error!("{op} failed: {e}");
                Variant::nil()
            })
        })
        .bind()
        .wait()
    }

    /// Parse the `create` config Dictionary into a core `ChatConfig` plus the
    /// `use_gpu` flag (used only when the model is given as a path). Errors
    /// on any recognized key holding a value of the wrong type.
    fn parse_config(
        config: &VarDictionary,
        reentrancy_flag: &Arc<AtomicBool>,
    ) -> Result<(nobodywho::chat::ChatConfig, bool), String> {
        let defaults = nobodywho::chat::ChatConfig::default();
        let tools = dict_get::<VarArray>(config, "tools")?
            .map(|arr| build_core_tools(&arr, reentrancy_flag))
            .unwrap_or_default();
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
            tools,
            ..defaults
        };
        let use_gpu = dict_get::<bool>(config, "use_gpu")?.unwrap_or(true);
        Ok((chat_config, use_gpu))
    }
}

/// A `String`-or-null Variant -> `Option<String>`; `Err` for anything else.
fn opt_string(v: &Variant) -> Result<Option<String>, ()> {
    if v.get_type() == godot::builtin::VariantType::NIL {
        Ok(None)
    } else {
        v.try_to::<GString>()
            .map(|s| Some(s.to_string()))
            .map_err(|_| ())
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

/// Build core `Tool`s from a GDScript Array of `Gd<NobodyWhoTool>`.
/// GDScript tools spawn their per-registration main-thread loop here (on the
/// main thread, which is where this runs) and capture this chat's
/// re-entrancy flag; built-in tools pass through. Tools that failed to build
/// are skipped with a `godot_error!` so one bad tool doesn't abort the set.
fn build_core_tools(
    tools: &VarArray,
    reentrancy_flag: &Arc<AtomicBool>,
) -> Vec<nobodywho::tool_calling::Tool> {
    tools
        .iter_shared()
        .filter_map(|v| match v.try_to::<Gd<NobodyWhoTool>>() {
            Ok(gd) => {
                let tool = gd.bind().build_core_tool(reentrancy_flag.clone());
                if tool.is_none() {
                    godot_error!("tools: skipped a tool that failed to build");
                }
                tool
            }
            Err(_) => {
                godot_error!("tools: element is not a NobodyWhoTool");
                None
            }
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
