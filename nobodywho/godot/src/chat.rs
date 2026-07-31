use std::rc::Rc;
use std::sync::Arc;

use godot::prelude::*;

use crate::convert::resolve_godot_path;
use crate::model::NobodyWhoModel;
use crate::task::{NobodyWhoTask, on_blocking_thread, task};

/// A chat session over a loaded model. Cheap to share (internally `Arc`).
///
/// Build it with the async factory:
/// ```gdscript
/// var chat = await NobodyWhoChat.create(model, {}).wait()
/// ```
/// `model` is a `NobodyWhoModel` or a path String (loaded with default options).
/// Resolves to the chat, or null on failure (with a `godot_error!`).
#[derive(GodotClass)]
#[class(no_init, base=RefCounted)]
pub struct NobodyWhoChat {
    handle: nobodywho::chat::ChatHandleAsync,
    base: Base<RefCounted>,
}

#[godot_api]
impl NobodyWhoChat {
    /// Create a chat asynchronously. Worker init runs off the main thread.
    /// `await create(...).wait()` resolves to a NobodyWhoChat, or null.
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
    ///
    /// Pass `{}` for defaults. Unrecognized keys are ignored.
    #[func]
    fn create(model: Variant, config: VarDictionary) -> Gd<NobodyWhoTask> {
        let system_prompt = config
            .get("system_prompt")
            .and_then(|v| v.try_to::<GString>().ok())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());
        let n_ctx = config
            .get("n_ctx")
            .and_then(|v| v.try_to::<i64>().ok())
            .map(|i| i.max(0) as u32);
        let n_threads = config
            .get("n_threads")
            .and_then(|v| v.try_to::<i64>().ok())
            .map(|i| i.max(0) as u32);
        let use_gpu = config
            .get("use_gpu")
            .and_then(|v| v.try_to::<bool>().ok())
            .unwrap_or(true);
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
            let defaults = nobodywho::chat::ChatConfig::default();
            let chat_config = nobodywho::chat::ChatConfig {
                system_prompt,
                n_ctx: n_ctx.unwrap_or(defaults.n_ctx),
                n_threads,
                ..defaults
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
