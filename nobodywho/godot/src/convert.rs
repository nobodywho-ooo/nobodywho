use godot::classes::ProjectSettings;
use godot::prelude::*;

/// Turn a GDScript path into something core can open.
/// `res://` and `user://` become real filesystem paths; everything else
/// (absolute paths, `huggingface:`, `hf://`, `http(s)://`, ...) is passed
/// through for core's path parser to handle.
pub fn resolve_godot_path(path: &GString) -> String {
    let s = path.to_string();
    if s.starts_with("res://") || s.starts_with("user://") {
        ProjectSettings::singleton().globalize_path(path).into()
    } else {
        s
    }
}

/// Globalize Godot paths in every image and audio part of a message list.
pub fn globalize_message_media_paths(
    messages: &mut [nobodywho::chat::Message],
) -> Result<(), String> {
    messages.iter_mut().try_for_each(|message| {
        message
            .content_mut()
            .media_parts_mut()
            .into_iter()
            .try_for_each(|part| {
                let path = match part {
                    nobodywho::chat::ContentPart::Image { path, .. }
                    | nobodywho::chat::ContentPart::Audio { path, .. } => path,
                    nobodywho::chat::ContentPart::Text { .. } => unreachable!(),
                };
                let path_string = path
                    .to_str()
                    .ok_or_else(|| format!("media path {} is not valid UTF-8", path.display()))?;
                if path_string.starts_with("res://") || path_string.starts_with("user://") {
                    *path = resolve_godot_path(&GString::from(path_string)).into();
                }
                Ok(())
            })
    })
}

/// Reject non-string and unknown configuration keys.
pub fn validate_config_keys(dict: &VarDictionary, allowed: &[&str]) -> Result<(), String> {
    dict.iter_shared().try_for_each(|(key, _)| {
        let key = key
            .try_to::<GString>()
            .map_err(|_| format!("config key {key} is not a String"))?
            .to_string();
        if allowed.contains(&key.as_str()) {
            Ok(())
        } else {
            Err(format!(
                "unknown config key \"{key}\"; expected one of: {}",
                allowed.join(", ")
            ))
        }
    })
}

/// Typed config-dictionary lookup. A missing key is fine (`Ok(None)`); a
/// present value of the wrong type is a hard error, so a mistyped config
/// value never silently falls back to the default.
pub fn dict_get<T: FromGodot>(dict: &VarDictionary, key: &str) -> Result<Option<T>, String> {
    dict.get(key).map_or(Ok(None), |v| {
        v.try_to::<T>().map(Some).map_err(|_| {
            format!(
                "config key \"{key}\" has unexpected type {:?}",
                v.get_type()
            )
        })
    })
}

/// Read a non-negative Godot integer that fits in `u32`.
pub fn dict_get_u32(dict: &VarDictionary, key: &str) -> Result<Option<u32>, String> {
    dict_get::<i64>(dict, key)?
        .map(|value| {
            u32::try_from(value).map_err(|_| {
                format!(
                    "config key \"{key}\" must be between 0 and {}, got {value}",
                    u32::MAX
                )
            })
        })
        .transpose()
}

/// Read a positive Godot integer that fits in `u32`.
pub fn dict_get_positive_u32(dict: &VarDictionary, key: &str) -> Result<Option<u32>, String> {
    dict_get_u32(dict, key)?.map_or(Ok(None), |value| {
        if value > 0 {
            Ok(Some(value))
        } else {
            Err(format!(
                "config key \"{key}\" must be a positive integer, got {value}"
            ))
        }
    })
}

/// Convert every element of an Array to a String, reporting the first bad
/// element instead of panicking through `Variant::to`.
pub fn collect_strings(values: &VarArray, name: &str) -> Result<Vec<String>, String> {
    values
        .iter_shared()
        .enumerate()
        .map(|(index, value)| {
            value
                .try_to::<GString>()
                .map(|string| string.to_string())
                .map_err(|_| {
                    format!(
                        "{name}[{index}] must be a String, got {:?}",
                        value.get_type()
                    )
                })
        })
        .collect()
}

// --- JSON <-> Variant bridge -----------------------------------------------
// Recursive converters between serde_json::Value and Godot Variant. Used for
// chat history (Vec<Message> serializes to a JSON array of role/content dicts).
// Godot has no pythonize equivalent, so we hand-roll the mapping.

/// Convert a `serde_json::Value` into a Godot `Variant`.
///
/// object -> Dictionary, array -> Array, string -> GString, number -> i64/f64,
/// bool -> bool, null -> nil.
pub fn json_to_variant(value: &serde_json::Value) -> Variant {
    use serde_json::Value;
    match value {
        Value::Null => Variant::nil(),
        Value::Bool(b) => b.to_variant(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_variant()
            } else if let Some(u) = n.as_u64() {
                (u as i64).to_variant()
            } else {
                n.as_f64().unwrap_or_default().to_variant()
            }
        }
        Value::String(s) => GString::from(s).to_variant(),
        Value::Array(arr) => {
            let mut out: VarArray = Array::new();
            for v in arr {
                out.push(&json_to_variant(v));
            }
            out.to_variant()
        }
        Value::Object(obj) => {
            let mut out: VarDictionary = Dictionary::new();
            for (k, v) in obj {
                let _ = out.insert(&GString::from(k), &json_to_variant(v));
            }
            out.to_variant()
        }
    }
}

/// Convert a Godot `Variant` into a `serde_json::Value`.
///
/// The inverse of [`json_to_variant`]. Used to parse GDScript chat-history
/// dicts back into `Vec<Message>` via `serde_json::from_value`.
pub fn variant_to_json(value: &Variant) -> Result<serde_json::Value, String> {
    use godot::builtin::VariantType;
    match value.get_type() {
        VariantType::NIL => Ok(serde_json::Value::Null),
        VariantType::BOOL => Ok(serde_json::Value::Bool(value.to::<bool>())),
        VariantType::INT => Ok(serde_json::Value::Number(value.to::<i64>().into())),
        VariantType::FLOAT => {
            let f = value.to::<f64>();
            serde_json::Number::from_f64(f)
                .map(serde_json::Value::Number)
                .ok_or_else(|| format!("float {f} is not JSON-representable"))
        }
        VariantType::STRING => Ok(serde_json::Value::String(value.to::<GString>().to_string())),
        VariantType::ARRAY => {
            // AnyArray, not VarArray: gdext 0.5 refuses to convert *typed*
            // arrays (e.g. GDScript's `Array[String]`) into `Array<Variant>`
            // ("expected array of type Untyped, got ..."). AnyArray is the
            // type-erased view that accepts both.
            let arr = value.to::<godot::builtin::AnyArray>();
            let mut out = Vec::with_capacity(arr.len());
            for v in arr.iter_shared() {
                out.push(variant_to_json(&v)?);
            }
            Ok(serde_json::Value::Array(out))
        }
        VariantType::DICTIONARY => {
            let dict = value.to::<VarDictionary>();
            let mut map = serde_json::Map::new();
            for (k, v) in dict.iter_shared() {
                let key = k
                    .try_to::<GString>()
                    .map_err(|_| format!("dictionary key {k} is not a String"))?;
                map.insert(key.to_string(), variant_to_json(&v)?);
            }
            Ok(serde_json::Value::Object(map))
        }
        other => Err(format!("variant type {other:?} has no JSON mapping")),
    }
}
