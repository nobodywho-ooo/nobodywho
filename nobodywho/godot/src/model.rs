use std::sync::Arc;

use godot::prelude::*;

use crate::convert::{dict_get, resolve_godot_path, validate_config_keys};
use crate::task::task;

/// A loaded model. Cheap to share (internally `Arc`); pass it to
/// `NobodyWhoChat.create` to start a chat.
///
/// Build it with the async factory:
/// ```gdscript
/// var model = await NobodyWhoModel.create("res://model.gguf", {})
/// ```
/// Resolves to the model, or null on failure (with a `godot_error!`).
///
/// Await the return value of `create` immediately (as above). Storing it and
/// awaiting after another await/frame is unsupported and may hang.
#[derive(GodotClass)]
#[class(no_init, base=RefCounted)]
pub struct NobodyWhoModel {
    pub(crate) inner: Arc<nobodywho::llm::Model>,
    base: Base<RefCounted>,
}

#[godot_api]
impl NobodyWhoModel {
    /// Load a model asynchronously. `await create(...)` resolves to a
    /// NobodyWhoModel, or null on failure. Await it immediately.
    ///
    /// `config` is a Dictionary with optional keys:
    /// - `"use_gpu"` (bool, default true): offload to GPU when available.
    /// - `"mmproj_path"` (String): multimodal projector file for vision models.
    /// - `"draft_path"` (String): MTP draft-heads gguf for speculative decoding.
    ///
    /// Pass `{}` for defaults. Unknown keys and values of the wrong type are
    /// errors (resolve to null).
    #[func]
    fn create(path: GString, config: VarDictionary) -> Variant {
        let path = resolve_godot_path(&path);
        let (use_gpu, mmproj_path, draft_path) = match Self::parse_config(&config) {
            Ok(parsed) => parsed,
            Err(e) => {
                godot_error!("NobodyWhoModel.create: {e}");
                return Variant::nil();
            }
        };
        // TODO: expose `progress` (download progress) as a Godot signal on the
        // task/model, not a GDScript Callable param. Core takes
        // Option<DownloadProgressCallback>; emit a signal from inside the
        // callback instead of bridging a Callable. Deferred to a later chunk.
        task(async move {
            match nobodywho::llm::get_model_async(path, use_gpu, mmproj_path, draft_path, None)
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
        .bind()
        .wait()
    }
}

impl NobodyWhoModel {
    /// Parse the `create` config Dictionary into `(use_gpu, mmproj_path,
    /// draft_path)`. Unknown keys and invalid values are errors.
    fn parse_config(
        config: &VarDictionary,
    ) -> Result<(bool, Option<String>, Option<String>), String> {
        validate_config_keys(config, &["use_gpu", "mmproj_path", "draft_path"])?;
        Ok((
            dict_get::<bool>(config, "use_gpu")?.unwrap_or(true),
            dict_get::<GString>(config, "mmproj_path")?.map(|s| resolve_godot_path(&s)),
            dict_get::<GString>(config, "draft_path")?.map(|s| resolve_godot_path(&s)),
        ))
    }
}
