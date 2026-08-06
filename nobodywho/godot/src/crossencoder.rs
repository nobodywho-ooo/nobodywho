use std::sync::Arc;

use godot::builtin::PackedFloat32Array;
use godot::prelude::*;

use nobodywho::crossencoder::CrossEncoderAsync;

use crate::convert::{dict_get, resolve_godot_path};
use crate::model::NobodyWhoModel;
use crate::task::task;

/// A cross-encoder reranker. Scores how relevant each document is to a
/// query (more accurate than embedding cosine-similarity, at higher compute
/// cost). Build it with the async factory:
///
/// ```gdscript
/// var ce = await NobodyWhoCrossEncoder.create(reranker_model, {})
/// var scores: PackedFloat32Array = await ce.rank("query", ["doc1", "doc2"])
/// var ranked: Array = await ce.rank_and_sort("query", docs)  # [{doc, score}, ...] desc
/// ```
///
/// `model` is a `NobodyWhoModel` or a path String. Resolves to the
/// `NobodyWhoCrossEncoder`, or null on failure (with a `godot_error!`).
#[derive(GodotClass)]
#[class(no_init, base=RefCounted)]
pub struct NobodyWhoCrossEncoder {
    handle: CrossEncoderAsync,
    base: Base<RefCounted>,
}

#[godot_api]
impl NobodyWhoCrossEncoder {
    /// Create a reranker asynchronously (the model loads off the main thread
    /// if a path is given). `await create(...)` resolves to the reranker, or
    /// null. `config` is a Dictionary with optional keys:
    /// - `"n_ctx"` (int): context window / max sequence length (default 4096).
    #[func]
    fn create(model: Variant, config: VarDictionary) -> Variant {
        let n_ctx = dict_get::<i64>(&config, "n_ctx")
            .ok()
            .flatten()
            .map(|i| i.max(0) as u32)
            .unwrap_or(4096);
        task(async move {
            // Resolve model-or-path, mirroring NobodyWhoChat.create.
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
                godot_error!(
                    "NobodyWhoCrossEncoder.create() expects a NobodyWhoModel or a path String"
                );
                return Variant::nil();
            };
            // CrossEncoderAsync::new is non-blocking (spawns a worker thread).
            Gd::from_init_fn(|base| Self {
                handle: CrossEncoderAsync::new(arc, n_ctx),
                base,
            })
            .to_variant()
        })
        .bind()
        .wait()
    }

    /// Score each document's relevance to the query. Returns a
    /// `PackedFloat32Array` with one score per document, in input order.
    /// `await rank(...)` resolves to the array, or null on failure.
    #[func]
    fn rank(&self, query: GString, documents: VarArray) -> Variant {
        let handle = self.handle.clone();
        let query = query.to_string();
        let docs: Vec<String> = documents
            .iter_shared()
            .map(|v| v.to::<GString>().to_string())
            .collect();
        task(async move {
            match handle.rank(query, docs).await {
                Ok(scores) => PackedFloat32Array::from(scores).to_variant(),
                Err(e) => {
                    godot_error!("rank failed: {e}");
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Score and sort documents by relevance to the query (highest first).
    /// `await rank_and_sort(...)` resolves to an `Array` of Dictionaries
    /// `{"doc": String, "score": float}` sorted by score descending, or null
    /// on failure.
    #[func]
    fn rank_and_sort(&self, query: GString, documents: VarArray) -> Variant {
        let handle = self.handle.clone();
        let query = query.to_string();
        let docs: Vec<String> = documents
            .iter_shared()
            .map(|v| v.to::<GString>().to_string())
            .collect();
        task(async move {
            match handle.rank_and_sort(query, docs).await {
                Ok(ranked) => {
                    let mut arr: VarArray = Array::new();
                    for (doc, score) in ranked {
                        let mut d: VarDictionary = Dictionary::new();
                        let _ = d.insert(&GString::from("doc"), &GString::from(&doc).to_variant());
                        let _ = d.insert(&GString::from("score"), &score.to_variant());
                        arr.push(&d.to_variant());
                    }
                    arr.to_variant()
                }
                Err(e) => {
                    godot_error!("rank_and_sort failed: {e}");
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }
}
