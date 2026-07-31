use std::sync::Arc;

use godot::prelude::*;

use crate::convert::{resolve_godot_path, resolve_optional_path};
use crate::model::NobodyWhoModel;
use crate::task::{on_blocking_thread, task, NobodyWhoTask};

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
            .and_then(|s| resolve_optional_path(&s));
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
            let mut chat_config = nobodywho::chat::ChatConfig {
                system_prompt,
                ..Default::default()
            };
            if let Some(n_ctx) = n_ctx {
                chat_config.n_ctx = n_ctx;
            }
            chat_config.n_threads = n_threads;
            // ChatHandleAsync::new blocks on worker init (sync channel recv),
            // so run it off the main thread.
            let result = on_blocking_thread(move || {
                nobodywho::chat::ChatHandleAsync::new(arc, chat_config)
            })
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

    /// Stop the current generation early. Chat-scoped: with queued concurrent
    /// asks, stops whatever is currently generating.
    #[func]
    fn stop_generation(&self) {
        self.handle.stop_generation();
    }
}
