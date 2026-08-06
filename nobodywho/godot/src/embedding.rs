use std::sync::Arc;

use godot::builtin::PackedFloat32Array;
use godot::prelude::*;

use nobodywho::encoder::{cosine_similarity, EncoderAsync};

use crate::convert::{dict_get, resolve_godot_path};
use crate::model::NobodyWhoModel;
use crate::task::task;

/// An embedding model. Converts text into float vectors (embeddings) for
/// semantic search / RAG. Build it with the async factory:
///
/// ```gdscript
/// var enc = await NobodyWhoEmbedding.create(model, {})
/// # or from a path (loaded with default model options):
/// var enc = await NobodyWhoEmbedding.create("res://bge-small.gguf", {})
/// var vec: PackedFloat32Array = await enc.encode("hello")
/// ```
///
/// `model` is a `NobodyWhoModel` or a path String. Resolves to the
/// `NobodyWhoEmbedding`, or null on failure (with a `godot_error!`).
#[derive(GodotClass)]
#[class(no_init, base=RefCounted)]
pub struct NobodyWhoEmbedding {
    handle: EncoderAsync,
    base: Base<RefCounted>,
}

#[godot_api]
impl NobodyWhoEmbedding {
    /// Create an embedding model asynchronously (the model loads off the main
    /// thread if a path is given). `await create(...)` resolves to the
    /// encoder, or null. `config` is a Dictionary with optional keys:
    /// - `"n_ctx"` (int): context window / max sequence length (default 4096).
    ///   Embedding models with longer max sequence lengths use more VRAM.
    #[func]
    fn create(model: Variant, config: VarDictionary) -> Variant {
        let n_ctx = dict_get::<i64>(&config, "n_ctx")
            .ok()
            .flatten()
            .map(|i| i.max(0) as u32)
            .unwrap_or(4096);
        task(async move {
            // Resolve model-or-path to a shared Arc<Model>, mirroring
            // NobodyWhoChat.create. A path is loaded with default options
            // (use_gpu=true); for richer control, load a NobodyWhoModel first.
            let arc = if let Ok(m) = model.try_to::<Gd<NobodyWhoModel>>() {
                m.bind().inner.clone()
            } else if let Ok(path) = model.try_to::<GString>() {
                let path = resolve_godot_path(&path);
                match nobodywho::llm::get_model_async(path, true, None, None, None).await {
                    Ok(m) => Arc::new(m),
                    Err(e) => {
                        godot_error!("Failed to load model: {}", nobodywho::render_miette(&e));
                        return Variant::nil();
                    }
                }
            } else {
                godot_error!("NobodyWhoEmbedding.create() expects a NobodyWhoModel or a path String");
                return Variant::nil();
            };
            // EncoderAsync::new is non-blocking (spawns a worker thread).
            Gd::from_init_fn(|base| Self {
                handle: EncoderAsync::new(arc, n_ctx),
                base,
            })
            .to_variant()
        })
        .bind()
        .wait()
    }

    /// Encode a single text into an embedding vector. `await encode(...)`
    /// resolves to a `PackedFloat32Array`, or null on failure.
    #[func]
    fn encode(&self, text: GString) -> Variant {
        let handle = self.handle.clone();
        let text = text.to_string();
        task(async move {
            match handle.encode(text).await {
                Ok(vec) => PackedFloat32Array::from(vec).to_variant(),
                Err(e) => {
                    godot_error!("encode failed: {e}");
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Encode multiple texts into embedding vectors, in input order. `await
    /// encode_batch(...)` resolves to an `Array` of `PackedFloat32Array`
    /// (one per input text), or null on failure.
    #[func]
    fn encode_batch(&self, texts: VarArray) -> Variant {
        let handle = self.handle.clone();
        let texts: Vec<String> = texts
            .iter_shared()
            .map(|v| v.to::<GString>().to_string())
            .collect();
        task(async move {
            match handle.encode_batch(texts).await {
                Ok(vecs) => {
                    let mut arr: VarArray = Array::new();
                    for v in vecs {
                        arr.push(&PackedFloat32Array::from(v).to_variant());
                    }
                    arr.to_variant()
                }
                Err(e) => {
                    godot_error!("encode_batch failed: {e}");
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Cosine similarity between two embedding vectors (static — no model
    /// needed). Returns a float in `[-1.0, 1.0]` (1.0 = identical). Pure
    /// math; resolves immediately.
    #[func]
    fn cosine_similarity(a: PackedFloat32Array, b: PackedFloat32Array) -> f32 {
        cosine_similarity(a.as_slice(), b.as_slice())
    }
}
