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
