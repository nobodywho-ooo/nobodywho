use std::sync::Arc;

use godot::prelude::*;

use nobodywho::decision::DecisionAsync;
use nobodywho::worker::AsyncWorker;

use crate::convert::{
    dict_get_positive_u32, json_to_variant, resolve_godot_path, validate_config_keys,
    variant_to_json,
};
use crate::model::NobodyWhoModel;
use crate::task::task;

/// Typed, non-generative decisions with a Laya model. Every answer comes with
/// probabilities instead of generated text. Build it with the async factory:
///
/// ```gdscript
/// var decision = await NobodyWhoDecision.create(model, {})
/// var response = await decision.decide(
///     "I was charged twice. Please refund the duplicate payment.",
///     {"refund": {"type": "noul", "instructions": "Is a refund requested?"}}
/// )
/// print(response["answers"]["refund"])
/// ```
///
/// `model` is a `NobodyWhoModel` or a path String. Resolves to the
/// `NobodyWhoDecision`, or null on failure (with a `godot_error!`).
#[derive(GodotClass)]
#[class(no_init, base=RefCounted)]
pub struct NobodyWhoDecision {
    handle: DecisionAsync,
    base: Base<RefCounted>,
}

#[godot_api]
impl NobodyWhoDecision {
    /// Create a decision handle asynchronously (the model loads off the main
    /// thread if a path is given). `await create(...)` resolves to the handle,
    /// or null. `config` is a Dictionary with optional keys:
    /// - `"n_ctx"` (int): context window (default 1024).
    #[func]
    fn create(model: Variant, config: VarDictionary) -> Variant {
        let n_ctx = match validate_config_keys(&config, &["n_ctx"])
            .and_then(|()| dict_get_positive_u32(&config, "n_ctx"))
        {
            Ok(n_ctx) => n_ctx.unwrap_or(1024),
            Err(e) => {
                godot_error!("NobodyWhoDecision.create: {e}");
                return Variant::nil();
            }
        };
        task(async move {
            let worker = AsyncWorker::new();
            // Resolve model-or-path, mirroring NobodyWhoEncoder.create.
            let arc = if let Ok(m) = model.try_to::<Gd<NobodyWhoModel>>() {
                m.bind().inner.clone()
            } else if let Ok(path) = model.try_to::<GString>() {
                let path = resolve_godot_path(&path);
                match worker
                    .run(move || nobodywho::llm::get_model(&path, true, None, None, None))
                    .await
                {
                    Ok(m) => Arc::new(m),
                    Err(e) => {
                        godot_error!("Failed to load model: {}", nobodywho::render_miette(&e));
                        return Variant::nil();
                    }
                }
            } else {
                godot_error!(
                    "NobodyWhoDecision.create() expects a NobodyWhoModel or a path String"
                );
                return Variant::nil();
            };
            // LayaWorker::new allocates the llama context before spawning the worker.
            match worker.run(move || DecisionAsync::new(arc, n_ctx)).await {
                Ok(handle) => Gd::from_init_fn(|base| Self { handle, base }).to_variant(),
                Err(e) => {
                    godot_error!(
                        "Failed to create decision handle: {}",
                        nobodywho::render_miette(&e)
                    );
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Run the questions over a state. `state` is a String, Dictionary, or
    /// Array; `questions` is a Dictionary mapping question names to
    /// `{"type": "choice" | "score" | "noul", "instructions": ..., "criteria": ...}`.
    /// `await decide(...)` resolves to a Dictionary with `"answers"` and
    /// `"usage"`, or null on failure.
    #[func]
    fn decide(&self, state: Variant, questions: VarDictionary) -> Variant {
        let state = match variant_to_json(&state) {
            Ok(state) => state,
            Err(e) => {
                godot_error!("decide: state {e}");
                return Variant::nil();
            }
        };
        let questions = match variant_to_json(&questions.to_variant()).and_then(|questions| {
            serde_json::from_value(serde_json::json!({ "state": state, "questions": questions }))
                .map_err(|e| format!("questions {e}"))
        }) {
            Ok(request) => request,
            Err(e) => {
                godot_error!("decide: {e}");
                return Variant::nil();
            }
        };
        let handle = self.handle.clone();
        task(async move {
            match handle.decide(questions).await {
                Ok(response) => match serde_json::to_value(&response) {
                    Ok(json) => json_to_variant(&json),
                    Err(e) => {
                        godot_error!("decide: {e}");
                        Variant::nil()
                    }
                },
                Err(e) => {
                    godot_error!("decide failed: {}", nobodywho::render_miette(&e));
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }
}
