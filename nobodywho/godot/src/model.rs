use std::sync::Arc;

use godot::prelude::*;

use crate::convert::{resolve_godot_path, resolve_optional_path};
use crate::task::{NobodyWhoTask, task};

/// A loaded model. Cheap to share (internally `Arc`); pass it to
/// `NobodyWhoChat.create` to start a chat.
///
/// Build it with the async factory:
/// ```gdscript
/// var model = await NobodyWhoModel.create("res://model.gguf", {}).wait()
/// ```
/// Resolves to the model, or null on failure (with a `godot_error!`).
#[derive(GodotClass)]
#[class(no_init, base=RefCounted)]
pub struct NobodyWhoModel {
    pub(crate) inner: Arc<nobodywho::llm::Model>,
    base: Base<RefCounted>,
}

#[godot_api]
impl NobodyWhoModel {
    /// Load a model asynchronously. `await create(...).wait()` resolves to a
    /// NobodyWhoModel, or null on failure.
    ///
    /// `config` is a Dictionary with optional keys:
    /// - `"use_gpu"` (bool, default true): offload to GPU when available.
    /// - `"mmproj_path"` (String): multimodal projector file for vision models.
    /// - `"draft_path"` (String): MTP draft-heads gguf for speculative decoding.
    ///
    /// Pass `{}` for defaults. Unrecognized keys are ignored.
    #[func]
    fn create(path: GString, config: VarDictionary) -> Gd<NobodyWhoTask> {
        let path = resolve_godot_path(&path);
        let use_gpu = config.get("use_gpu").and_then(|v| v.try_to::<bool>().ok());
        let mmproj_path = config
            .get("mmproj_path")
            .and_then(|v| v.try_to::<GString>().ok())
            .as_ref()
            .and_then(resolve_optional_path);
        let draft_path = config
            .get("draft_path")
            .and_then(|v| v.try_to::<GString>().ok())
            .as_ref()
            .and_then(resolve_optional_path);
        // TODO: expose `progress` (download progress) as a Godot signal on the
        // task/model, not a GDScript Callable param. Core takes
        // Option<DownloadProgressCallback>; emit a signal from inside the
        // callback instead of bridging a Callable. Deferred to a later chunk.
        task(async move {
            match nobodywho::llm::get_model_async(
                path,
                use_gpu.unwrap_or(true),
                mmproj_path,
                draft_path,
                None,
            )
            .await
            {
                Ok(model) => {
                    let gd = Gd::from_init_fn(|base| NobodyWhoModel {
                        inner: Arc::new(model),
                        base,
                    });
                    gd.to_variant()
                }
                Err(e) => {
                    godot_error!("Failed to load model: {}", nobodywho::render_miette(&e));
                    Variant::nil()
                }
            }
        })
    }

    /// Sets the global NobodyWho log level. One of
    /// "TRACE", "DEBUG", "INFO", "WARN", "ERROR".
    #[func]
    fn set_log_level(level: GString) {
        crate::set_log_level(&level.to_string());
    }
}
