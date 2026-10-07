use std::fmt::Display;

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::event_stream::event::{
    ContentPartAddedEvent, ContentPartDoneEvent, EventKind, FunctionCallArgumentsDeltaEvent,
    FunctionCallArgumentsDoneEvent, McpCallArgumentsDeltaEvent, McpCallArgumentsDoneEvent,
    McpEvent, OutputIndex, OutputItemAddedEvent, OutputItemDoneEvent, OutputTextDeltaEvent,
    OutputTextDoneEvent, ReasoningTextDeltaEvent, ReasoningTextDoneEvent, StreamEvent,
};

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

impl ItemKind {
    pub fn item_type(&self) -> ItemType {
        match self {
            ItemKind::Reasoning { .. } => ItemType::Reasoning,
            ItemKind::FunctionCall { .. } => ItemType::FunctionCall,
            ItemKind::Message { .. } => ItemType::Message,
            ItemKind::McpCall { .. } => ItemType::McpCall,
        }
    }
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

    fn get_item(&self, output_index: OutputIndex) -> Option<&Item> {
        self.output.get(output_index.0)
    }

    fn get_item_mut(&mut self, output_index: OutputIndex) -> Option<&mut Item> {
        self.output.get_mut(output_index.0)
    }

    /// The item an event addresses, checked against the id the event carries.
    fn get_event_item_mut(
        &mut self,
        output_index: OutputIndex,
        item_id: &ItemId,
        event: &str,
    ) -> &mut Item {
        let item = self.get_item_mut(output_index).unwrap_or_else(|| {
            panic!("OutputItemAdded event must have been received before {event} event")
        });
        if *item_id != item.id {
            panic!(
                "{event} event received for item_id {}, but the current item has id {}",
                item_id, item.id
            );
        }
        item
    }

    /// The MCP call an event addresses.
    fn get_mcp_call_mut(
        &mut self,
        output_index: OutputIndex,
        item_id: &ItemId,
        event: &str,
    ) -> (&mut Status, &mut McpCallItem) {
        let item = self.get_event_item_mut(output_index, item_id, event);
        match &mut item.kind {
            ItemKind::McpCall(call) => (&mut item.status, call),
            kind => panic!(
                "{event} event received for item of kind {:?}, which is not an MCP call",
                kind.item_type()
            ),
        }
    }

    pub fn consume_event(&mut self, event: StreamEvent) {
        let StreamEvent {
            sequence_number: _,
            tokens: _,
            //raw: _,
            kind,
        } = event;

        assert_ne!(
            self.status,
            Status::Completed,
            "Event received for a response that is already completed"
        );

        match kind {
            EventKind::Created { .. } => {
                panic!("Already in the middle of a response; cannot create a new one")
            }
            EventKind::InProgress { response } => {
                self.status = Status::InProgress;
                if response != *self {
                    panic!(
                        "InProgress event received, but the response object differs: expected {:?}, got {:?}",
                        self, response
                    );
                }
            }
            EventKind::Completed { response } => {
                self.status = Status::Completed;
                // Usage only arrives with the terminal event.
                self.usage = response.usage.clone();
                if response != *self {
                    panic!(
                        "Completed event received, but the response object differs: expected {:?}, got {:?}",
                        self, response
                    );
                }
            }
            EventKind::Incomplete { response } => {
                self.status = Status::Incomplete;
                self.usage = response.usage.clone();
                self.incomplete_details = response.incomplete_details.clone();
                if response != *self {
                    panic!(
                        "Incomplete event received, but the response object differs: expected {:?}, got {:?}",
                        self, response
                    );
                }
            }
            EventKind::OutputItemAdded(OutputItemAddedEvent { output_index, item }) => {
                if self.output.len() == output_index.0 {
                    self.output.push(item);
                } else {
                    panic!(
                        "OutputItemAdded event received out of order: expected index {}, got {}",
                        self.output.len(),
                        output_index.0
                    );
                }
            }
            EventKind::OutputItemDone(OutputItemDoneEvent { output_index, item }) => {
                let current_item = self.get_item_mut(output_index).expect(
                    "OutputItemAdded event must have been received before OutputItemDone event",
                );
                if current_item.id != item.id {
                    panic!(
                        "OutputItemDone event received for item_id {}, but the current item has id {}",
                        item.id, current_item.id
                    );
                }
                current_item.status = item.status;
                // What an MCP call returned has no events of its own.
                if let (ItemKind::McpCall(current), ItemKind::McpCall(done)) =
                    (&mut current_item.kind, &item.kind)
                {
                    current.result = done.result.clone();
                }
                if current_item != &item {
                    panic!(
                        "OutputItemDone event received for item_id {}, but the current item differs: expected {:?}, got {:?}",
                        item.id, current_item, item
                    );
                }
            }
            EventKind::ContentPartAdded(ContentPartAddedEvent {
                item_id,
                output_index,
                content_index,
                part,
            }) => {
                let item = self.get_item_mut(output_index).expect(
                    "OutputItemAdded event must have been received before ContentPartAdded event",
                );
                if item_id != item.id {
                    panic!(
                        "ContentPartAdded event received for item_id {}, but the current item has id {}",
                        item_id, item.id
                    );
                }
                match item.kind {
                    ItemKind::Reasoning (ReasoningItem{ ref mut content, .. } )| ItemKind::Message (MessageItem{ role: _, ref mut content }) => {
                        if content.len() == content_index.0 {
                            content.push(part);
                        } else {
                            panic!(
                                "ContentPartAdded event received out of order: expected index {}, got {}",
                                content.len(),
                                content_index.0
                            );
                        }
                    },
                    ItemKind::FunctionCall (_) | ItemKind::McpCall(_) => panic!(
                        "ContentPartAdded event received for item of kind {:?}, which does not support content parts",
                        item.kind
                    ),
                }
            }
            EventKind::ContentPartDone(ContentPartDoneEvent {
                item_id,
                output_index,
                content_index,
                part: done_content,
            }) => {
                let item = self.get_item(output_index).expect(
                    "OutputItemAdded event must have been received before ContentPartDone event",
                );
                if item_id != item.id {
                    panic!(
                        "ContentPartDone event received for item_id {}, but the current item has id {}",
                        item_id, item.id
                    );
                }
                match item.kind {
                    ItemKind::Reasoning (ReasoningItem{ ref content, .. } )| ItemKind::Message (MessageItem{ role: _, ref content }) => {
                        let part = content.get(content_index.0).expect(
                            "ContentPartAdded event must have been received before ContentPartDone event",
                        );
                        if done_content != *part {panic!(
                            "ContentPartDone event received for content part at index {}, but the content does not match: expected '{:?}', got '{:?}'",
                            content_index.0, part.text, done_content.text
                        )}
                    },
                    ItemKind::FunctionCall (_) | ItemKind::McpCall(_) => panic!(
                        "ContentPartDone event received for item of kind {:?}, which does not support content parts",
                        item.kind
                    ),
                }
            }
            EventKind::OutputTextDelta(OutputTextDeltaEvent {
                item_id,
                output_index,
                content_index,
                delta,
            }) => {
                let item = self.get_item_mut(output_index).expect(
                    "OutputItemAdded event must have been received before OutputTextDelta event",
                );
                if item_id != item.id {
                    panic!(
                        "OutputTextDelta event received for item_id {}, but the current item has id {}",
                        item_id, item.id
                    );
                }
                match item.kind {
                    ItemKind::Reasoning (ReasoningItem{ ref mut content, .. } )| ItemKind::Message (MessageItem{ role: _, ref mut content }) => {
                        let part = content.get_mut(content_index.0).expect(
                            "ContentPartAdded event must have been received before OutputTextDelta event",
                        );
                        part.text.push_str(&delta);
                    },
                    ItemKind::FunctionCall (_) | ItemKind::McpCall(_) => panic!(
                        "OutputTextDelta event received for item of kind {:?}, which does not support content parts",
                        item.kind
                    ),
                }
            }
            EventKind::OutputTextDone(OutputTextDoneEvent {
                item_id,
                output_index,
                content_index,
                text,
            }) => {
                let item = self.get_item(output_index).expect(
                    "OutputItemAdded event must have been received before OutputTextDone event",
                );
                if item_id != item.id {
                    panic!(
                        "OutputTextDone event received for item_id {}, but the current item has id {}",
                        item_id, item.id
                    );
                }
                match item.kind {
                    ItemKind::Reasoning (ReasoningItem{ ref content, .. } )| ItemKind::Message (MessageItem{ role: _, ref content }) => {
                        let part = content.get(content_index.0).expect(
                            "ContentPartAdded event must have been received before OutputTextDone event",
                        );
                        if text != part.text {panic!(
                            "OutputTextDone event received for content part at index {}, but the content does not match: expected '{}', got '{}'",
                            content_index.0, part.text, text
                        )}
                    },
                    ItemKind::FunctionCall (_) | ItemKind::McpCall(_) => panic!(
                        "OutputTextDone event received for item of kind {:?}, which does not support content parts",
                        item.kind
                    ),
                }
            }
            EventKind::ReasoningTextDelta(ReasoningTextDeltaEvent {
                item_id,
                output_index,
                content_index,
                delta,
            }) => {
                let item = self.get_event_item_mut(output_index, &item_id, "ReasoningTextDelta");
                let kind = item.kind.item_type();
                match item.kind {
                    ItemKind::Reasoning (ReasoningItem{ ref mut content, .. } ) => {
                        let part = content.get_mut(content_index.0).expect(
                            "ContentPartAdded event must have been received before ReasoningTextDelta event",
                        );
                        part.text.push_str(&delta);
                    }
                    ItemKind::FunctionCall (_)
                    | ItemKind::Message (_)
                    | ItemKind::McpCall(_) => panic!(
                        "ReasoningTextDelta event received for item of kind {kind:?}, which does not reason"
                    ),
                }
            }
            EventKind::ReasoningTextDone(ReasoningTextDoneEvent {
                item_id,
                output_index,
                content_index,
                text,
            }) => {
                let item = self.get_event_item_mut(output_index, &item_id, "ReasoningTextDone");
                let kind = item.kind.item_type();
                match item.kind {
                    ItemKind::Reasoning (ReasoningItem{ ref content, .. } ) => {
                        let part = content.get(content_index.0).expect(
                            "ContentPartAdded event must have been received before ReasoningTextDone event",
                        );
                        if text != part.text {
                            panic!(
                                "ReasoningTextDone event received for content part at index {}, but the content does not match: expected '{}', got '{}'",
                                content_index.0, part.text, text
                            )
                        }
                    }
                    ItemKind::FunctionCall (_)
                    | ItemKind::Message (_)
                    | ItemKind::McpCall(_) => panic!(
                        "ReasoningTextDone event received for item of kind {kind:?}, which does not reason"
                    ),
                }
            }
            EventKind::FunctionCallArgumentsDelta(FunctionCallArgumentsDeltaEvent {
                item_id,
                output_index,
                delta,
            }) => {
                let item = self.get_item_mut(output_index).expect(
                    "OutputItemAdded event must have been received before FunctionCallArgumentsDelta event",
                );
                if item_id != item.id {
                    panic!(
                        "FunctionCallArgumentsDelta event received for item_id {}, but the current item has id {}",
                        item_id, item.id
                    );
                }
                match item.kind {
                    ItemKind::FunctionCall (
                        FunctionCallItem {
                            call_id: _,
                            name: _,
                            ref mut arguments,
                        }
                    ) => {arguments.push_str(&delta); },
                    ItemKind::Reasoning (_)
                    | ItemKind::Message (_)
                    | ItemKind::McpCall(_) => panic!(
                        "FunctionCallArgumentsDelta event received for item of kind {:?}, which does not support arguments",
                        item.kind.item_type()
                    ),
                }
            }
            EventKind::FunctionCallArgumentsDone(FunctionCallArgumentsDoneEvent {
                item_id,
                output_index,
                arguments,
            }) => {
                let item = self.get_item(output_index).expect(
                    "OutputItemAdded event must have been received before FunctionCallArgumentsDone event",
                );
                if item_id != item.id {
                    panic!(
                        "FunctionCallArgumentsDone event received for item_id {}, but the current item has id {}",
                        item_id, item.id
                    );
                }
                match &item.kind {
                    ItemKind::FunctionCall (
                        FunctionCallItem {
                            call_id: _,
                            name: _,
                            arguments: current_arguments,
                        }
                    ) => {
                        if *current_arguments != arguments {
                            panic!(
                                "FunctionCallArgumentsDone event received for item_id {}, but the arguments do not match: expected '{}', got '{}'",
                                item_id, current_arguments, arguments
                            );
                        }
                    }
                    ItemKind::Reasoning (_)
                    | ItemKind::Message (_)
                    | ItemKind::McpCall(_) => panic!(
                        "FunctionCallArgumentsDone event received for item of kind {:?}, which does not support arguments",
                        item.kind.item_type()
                    ),
                }
            }
            EventKind::McpCallArgumentsDelta(McpCallArgumentsDeltaEvent {
                item_id,
                output_index,
                delta,
            }) => {
                let (_, call) =
                    self.get_mcp_call_mut(output_index, &item_id, "McpCallArgumentsDelta");
                call.arguments.push_str(&delta);
            }
            EventKind::McpCallArgumentsDone(McpCallArgumentsDoneEvent {
                item_id,
                output_index,
                arguments,
            }) => {
                let (_, call) =
                    self.get_mcp_call_mut(output_index, &item_id, "McpCallArgumentsDone");
                if call.arguments != arguments {
                    panic!(
                        "McpCallArgumentsDone event received for item_id {}, but the arguments do not match: expected '{}', got '{}'",
                        item_id, call.arguments, arguments
                    );
                }
            }
            EventKind::McpCallInProgress(McpEvent {
                item_id,
                output_index,
            }) => {
                let (status, _) =
                    self.get_mcp_call_mut(output_index, &item_id, "McpCallInProgress");
                if *status != Status::InProgress {
                    panic!("McpCallInProgress event received for item_id {item_id}, which is {status:?}");
                }
            }
            EventKind::McpCallCompleted(McpEvent {
                item_id,
                output_index,
            }) => {
                let (status, _) = self.get_mcp_call_mut(output_index, &item_id, "McpCallCompleted");
                *status = Status::Completed;
            }
            EventKind::McpCallFailed(McpEvent {
                item_id,
                output_index,
            }) => {
                let (status, _) = self.get_mcp_call_mut(output_index, &item_id, "McpCallFailed");
                *status = Status::Failed;
            }
        }
    }
}
