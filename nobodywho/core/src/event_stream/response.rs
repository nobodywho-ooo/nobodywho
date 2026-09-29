use std::fmt::Display;

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResponseId(pub String);

impl ResponseId {
    pub fn generate(rng: &mut impl Rng) -> Self {
        ResponseId(format!("resp_{}", random_id_suffix(rng)))
    }
}

/// An identifier for an item. Unique within the entire conversation.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemId(pub String);

/// 128 random bits as hex, so a generated id is unique without having to
/// coordinate with anything.
fn random_id_suffix(rng: &mut impl Rng) -> String {
    format!("{:016x}{:016x}", rng.random::<u64>(), rng.random::<u64>())
}

impl ItemId {
    /// A fresh id for an item of this type.
    fn generate(item_type: ItemType, rng: &mut impl Rng) -> Self {
        ItemId(format!(
            "{}{}",
            item_type.id_prefix(),
            random_id_suffix(rng)
        ))
    }

    pub fn generate_reasoning(rng: &mut impl Rng) -> Self {
        ItemId::generate(ItemType::Reasoning, rng)
    }

    pub fn generate_function_call(rng: &mut impl Rng) -> Self {
        ItemId::generate(ItemType::FunctionCall, rng)
    }

    pub fn generate_message(rng: &mut impl Rng) -> Self {
        ItemId::generate(ItemType::Message, rng)
    }

    pub fn generate_mcp_call(rng: &mut impl Rng) -> Self {
        ItemId::generate(ItemType::McpCall, rng)
    }
}

impl Display for ItemId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ItemId(id) = self;
        write!(f, "{id}")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ContentPartIndex(pub usize);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FunctionCallId(pub String);

impl FunctionCallId {
    /// A fresh call id. This is the handle a tool result is matched back to, and
    /// is not the function call item's own [`ItemId`].
    pub fn generate(rng: &mut impl Rng) -> Self {
        FunctionCallId(format!("call_{}", random_id_suffix(rng)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
pub enum ContentPartType {
    OutputText,
    ReasoningText,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentPart {
    pub r#type: ContentPartType,
    pub text: String,
}

impl ContentPart {
    pub fn reasoning(text: String) -> Self {
        ContentPart {
            r#type: ContentPartType::ReasoningText,
            text,
        }
    }

    pub fn output(text: String) -> Self {
        ContentPart {
            r#type: ContentPartType::OutputText,
            text,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    InProgress,
    Completed,
    Incomplete,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemType {
    Reasoning,
    FunctionCall,
    Message,
    McpCall,
}

impl ItemType {
    /// The prefix the Responses API puts on an item id of this type.
    pub fn id_prefix(&self) -> &'static str {
        match self {
            ItemType::Reasoning => "rs_",
            ItemType::FunctionCall => "fc_",
            ItemType::Message => "msg_",
            ItemType::McpCall => "mcp_",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningItem {
    /// Always empty, as models here don't summarize their reasoning, but the
    /// API has every reasoning item carry it.
    #[serde(default)]
    pub summary: Vec<ContentPart>,
    #[serde(default)]
    pub content: Vec<ContentPart>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionCallItem {
    pub call_id: FunctionCallId,
    pub name: String,
    pub arguments: String,
}

impl FunctionCallItem {
    pub fn new(function_name: String, rng: &mut impl Rng) -> Self {
        FunctionCallItem {
            call_id: FunctionCallId::generate(rng),
            name: function_name,
            arguments: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageItem {
    pub role: Role,
    pub content: Vec<ContentPart>,
}

/// A call we run rather than the client, written as the API's MCP call item,
/// though no MCP server is involved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "McpCallFields", into = "McpCallFields")]
pub struct McpCallItem {
    pub server_label: String,
    pub name: String,
    pub arguments: String,
    /// What the call returned, which only arrives when it's done.
    pub result: Option<Result<String, McpCallError>>,
}

/// An MCP call as the wire has it, with its output and error side by side.
#[derive(Serialize, Deserialize)]
struct McpCallFields {
    server_label: String,
    name: String,
    arguments: String,
    output: Option<String>,
    error: Option<McpCallError>,
}

impl From<McpCallFields> for McpCallItem {
    fn from(fields: McpCallFields) -> Self {
        McpCallItem {
            server_label: fields.server_label,
            name: fields.name,
            arguments: fields.arguments,
            result: match (fields.output, fields.error) {
                (_, Some(error)) => Some(Err(error)),
                (output, None) => output.map(Ok),
            },
        }
    }
}

impl From<McpCallItem> for McpCallFields {
    fn from(item: McpCallItem) -> Self {
        let (output, error) = match item.result {
            None => (None, None),
            Some(Ok(output)) => (Some(output), None),
            Some(Err(error)) => (None, Some(error)),
        };
        McpCallFields {
            server_label: item.server_label,
            name: item.name,
            arguments: item.arguments,
            output,
            error,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum McpCallError {
    #[serde(rename = "mcp_protocol_error")]
    Protocol { code: i64, message: String },
    #[serde(rename = "mcp_tool_execution_error")]
    ToolExecution { content: serde_json::Value },
    #[serde(rename = "http_error")]
    Http { code: u16, message: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ItemKind {
    Reasoning(ReasoningItem),
    FunctionCall(FunctionCallItem),
    Message(MessageItem),
    McpCall(McpCallItem),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: ItemId,
    pub status: Status,
    #[serde(flatten)]
    pub kind: ItemKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseObject {
    pub(crate) id: ResponseId,
    pub(crate) usage: Option<ResponseUsage>,
    pub(crate) status: Status,
    /// Why an incomplete response is incomplete, when it's for a reason the
    /// API has a name for.
    pub(crate) incomplete_details: Option<IncompleteDetails>,
    pub(crate) output: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncompleteDetails {
    pub reason: IncompleteReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncompleteReason {
    MaxOutputTokens,
    ContentFilter,
}

impl ResponseObject {
    pub fn init(response_id: ResponseId) -> Self {
        ResponseObject {
            id: response_id,
            usage: None,
            status: Status::InProgress,
            incomplete_details: None,
            output: Vec::new(),
        }
    }
}
