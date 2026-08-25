//! Typed message content: interleaved text and media.

use core::fmt;
use std::path::{Path, PathBuf};

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

/// The `type` tags we claim. An array of objects carrying only these is read as
/// content parts; an array carrying none of them is passed through untouched.
const RESERVED_TYPES: [&str; 3] = ["text", "image", "audio"];

/// Forces a value through as raw JSON even when it looks like content parts:
/// `{"type": "raw", "value": …}`.
const RAW_TAG: &str = "raw";

/// One piece of a message: a run of text, or a media file embedded at this
/// position.
///
/// `id` is the bitmap a worker registered for the file. It is worker-local, so
/// content that came from elsewhere — a saved conversation, say — carries an id
/// this worker knows nothing about and must be re-registered from the path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ContentPart {
    Text {
        text: String,
    },
    Image {
        path: PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    Audio {
        path: PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
}

impl ContentPart {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    pub fn image(path: impl Into<PathBuf>) -> Self {
        Self::Image {
            path: path.into(),
            id: None,
        }
    }

    pub fn audio(path: impl Into<PathBuf>) -> Self {
        Self::Audio {
            path: path.into(),
            id: None,
        }
    }

    pub fn is_media(&self) -> bool {
        !matches!(self, Self::Text { .. })
    }

    /// The file this part was loaded from, or `None` for text.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Text { .. } => None,
            Self::Image { path, .. } | Self::Audio { path, .. } => Some(path.as_path()),
        }
    }

    /// The registered bitmap, or `None` for text and for media this worker has
    /// not registered yet.
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Text { .. } => None,
            Self::Image { id, .. } | Self::Audio { id, .. } => id.as_deref(),
        }
    }

    pub fn set_id(&mut self, new_id: String) {
        match self {
            Self::Text { .. } => {}
            Self::Image { id, .. } | Self::Audio { id, .. } => *id = Some(new_id),
        }
    }
}

/// The content of a message.
///
/// The variants are told apart by shape, so an OpenAI-style message array can be
/// passed in directly while chat templates that expect a structured `content`
/// keep working:
///
/// | JSON | variant |
/// |---|---|
/// | `"hello"` | [`Text`](Self::Text) |
/// | `[{"type": "text", …}, {"type": "image", …}]` | [`Parts`](Self::Parts) |
/// | `{"type": "raw", "value": V}` | [`Json`](Self::Json), holding `V` |
/// | anything else | [`Json`](Self::Json) |
///
/// `text`, `image` and `audio` are reserved. An array carrying none of them —
/// say `[{"type": "document", …}]`, for a model finetuned on structured turns —
/// reaches the chat template as a real list. Mixing the two, or using a reserved
/// tag with fields that do not parse, is an error rather than a silent
/// fallthrough, so a typo surfaces instead of being rendered as literal JSON.
///
/// Serialization is transparent, *except* that a [`Json`](Self::Json) which
/// would read back as something else is wrapped in `{"type": "raw", …}`.
/// Tagging only when necessary leaves the wire format unchanged for payloads
/// that do not collide, while guaranteeing content survives a round trip out of
/// the history and back in.
#[derive(Clone, Debug)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
    Json(Value),
}

impl MessageContent {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(text.into())
    }

    /// Build content from parts, merging adjacent text runs.
    pub fn parts(parts: impl IntoIterator<Item = ContentPart>) -> Self {
        Self::Parts(merge_adjacent_texts(parts))
    }

    pub fn from_json(value: Value) -> Self {
        Self::Json(value)
    }

    /// The media parts, in order. The nth of these pairs with the nth media
    /// marker in the flattened text.
    pub fn media_parts(&self) -> Vec<&ContentPart> {
        match self {
            Self::Parts(parts) => parts.iter().filter(|part| part.is_media()).collect(),
            Self::Text(_) | Self::Json(_) => vec![],
        }
    }

    pub fn media_parts_mut(&mut self) -> Vec<&mut ContentPart> {
        match self {
            Self::Parts(parts) => parts.iter_mut().filter(|part| part.is_media()).collect(),
            Self::Text(_) | Self::Json(_) => vec![],
        }
    }

    pub fn media_paths(&self) -> Vec<&Path> {
        self.media_parts()
            .into_iter()
            .filter_map(ContentPart::path)
            .collect()
    }

    /// Kept separate from [`Deserialize`] so [`Serialize`] can use it to decide
    /// whether a value needs the `raw` wrapper to survive the round trip.
    fn from_value(value: Value) -> Result<Self, String> {
        match value {
            Value::String(text) => Ok(Self::Text(text)),
            Value::Array(items) => match sniff_parts(&items)? {
                Some(parts) => Ok(Self::Parts(parts)),
                None => Ok(Self::Json(Value::Array(items))),
            },
            Value::Object(ref fields) if is_raw_wrapper(fields) => {
                Ok(Self::Json(fields["value"].clone()))
            }
            other => Ok(Self::Json(other)),
        }
    }
}

/// `Ok(Some(parts))` if every element carries a reserved tag and parses,
/// `Ok(None)` if none of them do, and `Err` for the in-between cases.
fn sniff_parts(items: &[Value]) -> Result<Option<Vec<ContentPart>>, String> {
    let claimed = items.iter().filter(|item| is_reserved(item)).count();

    if claimed == 0 && !items.is_empty() {
        return Ok(None);
    }

    if claimed != items.len() {
        return Err(format!(
            "content array mixes content parts with raw JSON ({claimed} of {} entries use one of \
             the reserved types {}). Wrap it as {{\"type\": \"{RAW_TAG}\", \"value\": [...]}} to \
             pass the whole array through to the chat template untouched.",
            items.len(),
            RESERVED_TYPES.join(", "),
        ));
    }

    items
        .iter()
        .map(|item| serde_json::from_value(item.clone()).map_err(|e| e.to_string()))
        .collect::<Result<Vec<ContentPart>, String>>()
        .map(Some)
}

fn merge_adjacent_texts(parts: impl IntoIterator<Item = ContentPart>) -> Vec<ContentPart> {
    parts.into_iter().fold(vec![], |mut acc, part| {
        match (acc.last_mut(), &part) {
            (Some(ContentPart::Text { text: last }), ContentPart::Text { text: next }) => {
                last.push_str(next);
            }
            _ => acc.push(part),
        }
        acc
    })
}

fn is_reserved(item: &Value) -> bool {
    item.get("type")
        .and_then(Value::as_str)
        .is_some_and(|tag| RESERVED_TYPES.contains(&tag))
}

fn is_raw_wrapper(fields: &serde_json::Map<String, Value>) -> bool {
    fields.len() == 2
        && fields.contains_key("value")
        && fields.get("type").and_then(Value::as_str) == Some(RAW_TAG)
}

/// Unwrap a `{"type": "raw", "value": V}` envelope in place. The envelope only
/// exists so history reads back unchanged; a template must see `V` itself.
pub(crate) fn strip_raw_wrapper(value: &mut Value) {
    let Value::Object(fields) = value else {
        return;
    };
    if !is_raw_wrapper(fields) {
        return;
    }
    *value = fields["value"].take();
}

/// True when writing the value plainly would not read back as the same thing —
/// an array of part-shaped objects, or a value that is itself a `raw` wrapper.
fn needs_raw_wrapper(value: &Value) -> bool {
    !matches!(
        MessageContent::from_value(value.clone()),
        Ok(MessageContent::Json(ref parsed)) if parsed == value
    )
}

impl Serialize for MessageContent {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Text(text) => text.serialize(s),
            Self::Parts(parts) => parts.serialize(s),
            Self::Json(value) if needs_raw_wrapper(value) => {
                serde_json::json!({ "type": RAW_TAG, "value": value }).serialize(s)
            }
            Self::Json(value) => value.serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for MessageContent {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::from_value(Value::deserialize(d)?).map_err(D::Error::custom)
    }
}

/// Flattens content into the text a chat template sees: text runs inline, each
/// media part replaced by the mtmd marker holding its position.
impl fmt::Display for MessageContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(text) => write!(f, "{text}"),
            Self::Json(value) => write!(f, "{value}"),
            Self::Parts(parts) => {
                let marker = llama_cpp_2::mtmd::mtmd_default_marker();
                for part in parts {
                    match part {
                        ContentPart::Text { text } => write!(f, "{text}")?,
                        _ => write!(f, "{marker}")?,
                    }
                }
                Ok(())
            }
        }
    }
}

impl Default for MessageContent {
    fn default() -> Self {
        Self::Parts(vec![])
    }
}

impl From<String> for MessageContent {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}

impl From<&str> for MessageContent {
    fn from(text: &str) -> Self {
        Self::Text(text.to_string())
    }
}

impl From<Vec<ContentPart>> for MessageContent {
    fn from(parts: Vec<ContentPart>) -> Self {
        Self::parts(parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(value: Value) -> Result<MessageContent, String> {
        MessageContent::from_value(value)
    }

    #[test]
    fn plain_string_is_text() {
        assert!(matches!(parse(json!("hello")).unwrap(), MessageContent::Text(t) if t == "hello"));
    }

    #[test]
    fn recognized_array_is_parts() {
        let content = parse(json!([
            {"type": "text", "text": "describe this"},
            {"type": "image", "path": "cat.png"},
        ]))
        .unwrap();

        let MessageContent::Parts(parts) = content else {
            panic!("expected parts");
        };
        assert_eq!(
            parts,
            vec![
                ContentPart::text("describe this"),
                ContentPart::image("cat.png"),
            ]
        );
    }

    #[test]
    fn foreign_array_passes_through_unwrapped() {
        // What `Json` exists for: a model finetuned on structured turns, whose
        // template iterates the array itself.
        let value = json!([
            {"type": "query", "text": "what is our refund policy?"},
            {"type": "document", "title": "Returns", "body": "…"},
        ]);
        let content = parse(value.clone()).unwrap();
        assert!(matches!(content, MessageContent::Json(ref v) if *v == value));
        assert_eq!(serde_json::to_value(&content).unwrap(), value);
    }

    #[test]
    fn raw_wrapper_forces_passthrough_and_round_trips() {
        // The escape hatch, for content that would otherwise read as parts.
        let inner = json!([{"type": "text", "text": "not ours"}]);
        let wrapped = json!({"type": RAW_TAG, "value": inner});

        let content = parse(wrapped.clone()).unwrap();
        assert!(matches!(content, MessageContent::Json(ref v) if *v == inner));
        // It must come back wrapped, or it would read as parts next time.
        assert_eq!(serde_json::to_value(&content).unwrap(), wrapped);
    }

    #[test]
    fn strip_raw_wrapper_undoes_the_envelope() {
        let inner = json!([{"type": "text", "text": "not ours"}]);
        let mut value = serde_json::to_value(MessageContent::Json(inner.clone())).unwrap();
        assert_ne!(value, inner, "this content should serialize wrapped");

        strip_raw_wrapper(&mut value);
        assert_eq!(value, inner);

        // Anything that is not an envelope is left alone.
        let mut plain = json!({"type": "document", "title": "Returns"});
        let untouched = plain.clone();
        strip_raw_wrapper(&mut plain);
        assert_eq!(plain, untouched);
    }

    #[test]
    fn mixed_array_is_an_error() {
        let err = parse(json!([
            {"type": "text", "text": "hi"},
            {"type": "document", "title": "Returns"},
        ]))
        .unwrap_err();
        assert!(err.contains(RAW_TAG), "{err}");
    }

    #[test]
    fn malformed_part_is_an_error_not_passthrough() {
        // A typo must surface, not get rendered into the prompt as literal JSON.
        assert!(parse(json!([{"type": "image", "url": "cat.png"}])).is_err());
    }

    #[test]
    fn flattening_puts_a_marker_where_each_media_part_was() {
        let marker = llama_cpp_2::mtmd::mtmd_default_marker();
        let content = MessageContent::Parts(vec![
            ContentPart::text("before "),
            ContentPart::image("cat.png"),
            ContentPart::text(" after"),
        ]);
        assert_eq!(content.to_string(), format!("before {marker} after"));
    }
}
