use godot::prelude::*;

use crate::convert::{resolve_godot_path, variant_to_json};

/// A multimodal prompt: interleaved text / image / audio parts, or a raw
/// JSON message. Immutable after construction.
///
/// Build it from an Array of part dicts produced by the static part
/// factories, then pass it to `NobodyWhoChat.ask` / `tokenize`:
/// ```gdscript
/// var prompt = NobodyWhoPrompt.create([
///     NobodyWhoPrompt.text("What is in this image?"),
///     NobodyWhoPrompt.image("res://images/photo.jpg"),
/// ])
/// var stream = chat.ask(prompt)
/// ```
///
/// For a raw chat-template message dict (no Godot path resolution), use
/// [`from_json`]:
/// ```gdscript
/// var prompt = NobodyWhoPrompt.from_json({"role": "user", "content": "Hello"})
/// ```
///
/// Mirrors the other bindings' immutable parts-array shape (Python, Kotlin,
/// Swift, Flutter, React Native). There are no `add_*` mutators.
#[derive(GodotClass)]
#[class(no_init, base=RefCounted)]
pub struct NobodyWhoPrompt {
    /// The finished core prompt, set once at construction. `Parts` when built
    /// via `create`, `Json` when built via `from_json`. Read directly by
    /// `chat.rs` as `gd.bind().inner.clone()` — same shape as
    /// `NobodyWhoModel::inner` / `NobodyWhoSamplerConfig::inner`.
    pub(crate) inner: nobodywho::tokenizer::Prompt,
    base: Base<RefCounted>,
}

#[godot_api]
impl NobodyWhoPrompt {
    /// A text part dict for [`create`]. Does no path resolution.
    #[func]
    fn text(content: GString) -> VarDictionary {
        part_dict("text", &content)
    }

    /// An image part dict for [`create`]. `path` is stored unresolved;
    /// [`create`] resolves `res://` / `user://` to real filesystem paths.
    #[func]
    fn image(path: GString) -> VarDictionary {
        part_dict("image", &path)
    }

    /// An audio part dict for [`create`]. `path` is stored unresolved;
    /// [`create`] resolves `res://` / `user://` to real filesystem paths.
    #[func]
    fn audio(path: GString) -> VarDictionary {
        part_dict("audio", &path)
    }

    /// Build a prompt from an Array of part dicts (as produced by
    /// [`text`] / [`image`] / [`audio`]). Each dict is `{"type": String,
    /// "value": String}` with `type` one of `"text"` / `"image"` / `"audio"`.
    ///
    /// Image/audio paths are resolved through `resolve_godot_path` here — the
    /// single path seam. Returns a `NobodyWhoPrompt` (as a Variant) on
    /// success, or `null` + `godot_error!` on the first bad element (unknown
    /// `"type"`, missing/wrong-typed `"value"`, or an element that isn't a
    /// Dictionary). Consistent with [`from_json`] — one failure convention
    /// for both sync factories.
    #[func]
    fn create(parts: VarArray) -> Variant {
        match parse_parts(&parts) {
            Ok(core_parts) => Gd::from_init_fn(|base| Self {
                inner: nobodywho::tokenizer::Prompt::new(core_parts),
                base,
            })
            .to_variant(),
            Err(e) => {
                godot_error!("NobodyWhoPrompt.create: {e}");
                Variant::nil()
            }
        }
    }

    /// Build a prompt from a JSON-compatible Godot value (Dictionary, Array,
    /// etc.). The value is stored as-is; **no** Godot path resolution is
    /// applied (paths inside JSON are whatever the user put in).
    ///
    /// Returns a `NobodyWhoPrompt` (as a Variant) on success, or `null` +
    /// `godot_error!` if the value can't be converted to JSON.
    #[func]
    fn from_json(data: Variant) -> Variant {
        match variant_to_json(&data) {
            Ok(value) => Gd::from_init_fn(|base| Self {
                inner: nobodywho::tokenizer::Prompt::from_json(value),
                base,
            })
            .to_variant(),
            Err(e) => {
                godot_error!("NobodyWhoPrompt.from_json: {e}");
                Variant::nil()
            }
        }
    }
}

/// Build a `{"type": ty, "value": value}` Dictionary for a part factory.
/// Pure value constructor — no resolution, no validation beyond the shape.
fn part_dict(ty: &str, value: &GString) -> VarDictionary {
    let mut d: VarDictionary = Dictionary::new();
    let _ = d.insert(&GString::from("type"), &GString::from(ty).to_variant());
    let _ = d.insert(&GString::from("value"), &value.to_variant());
    d
}

/// Parse an Array of part dicts into core `PromptPart`s, resolving image/
/// audio paths through `resolve_godot_path`. Returns `Err` on the first bad
/// element — `create` nulls on error (consistent with `from_json`).
fn parse_parts(parts: &VarArray) -> Result<Vec<nobodywho::tokenizer::PromptPart>, String> {
    parts.iter_shared().map(|v| parse_part(&v)).collect()
}

/// Convert one Array element into a core `PromptPart`, resolving image/audio
/// paths. `Err` for a non-Dictionary, unknown type, or missing/wrong field.
fn parse_part(v: &Variant) -> Result<nobodywho::tokenizer::PromptPart, String> {
    let d = v
        .try_to::<VarDictionary>()
        .map_err(|_| "part is not a Dictionary".to_string())?;
    let ty = d
        .get("type")
        .and_then(|x| x.try_to::<GString>().ok())
        .ok_or("part is missing a String \"type\" field")?;
    let value = d
        .get("value")
        .and_then(|x| x.try_to::<GString>().ok())
        .ok_or("part is missing a String \"value\" field")?;
    match ty.to_string().as_str() {
        "text" => Ok(nobodywho::tokenizer::PromptPart::Text(value.to_string())),
        "image" => Ok(nobodywho::tokenizer::PromptPart::Image(
            resolve_godot_path(&value).into(),
        )),
        "audio" => Ok(nobodywho::tokenizer::PromptPart::Audio(
            resolve_godot_path(&value).into(),
        )),
        other => Err(format!("unknown part type \"{other}\"")),
    }
}
