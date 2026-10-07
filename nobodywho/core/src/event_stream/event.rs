use serde::{Deserialize, Serialize};

use crate::output_format::Token;

use crate::event_stream::response::{ContentPart, ContentPartIndex, Item, ItemId, ResponseObject};

/// The item's index within the response.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OutputIndex(pub usize);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SequenceNumber(pub usize);

impl SequenceNumber {
    pub fn start() -> Self {
        SequenceNumber(0)
    }

    pub fn next(self) -> Self {
        SequenceNumber(self.0 + 1)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputItemAddedEvent {
    pub output_index: OutputIndex,
    pub item: Item,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputItemDoneEvent {
    pub output_index: OutputIndex,
    pub item: Item,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentPartAddedEvent {
    pub item_id: ItemId,
    pub output_index: OutputIndex,
    pub content_index: ContentPartIndex,
    pub part: ContentPart,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentPartDoneEvent {
    pub item_id: ItemId,
    pub output_index: OutputIndex,
    pub content_index: ContentPartIndex,
    pub part: ContentPart,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputTextDeltaEvent {
    pub item_id: ItemId,
    pub output_index: OutputIndex,
    pub content_index: ContentPartIndex,
    pub delta: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputTextDoneEvent {
    pub item_id: ItemId,
    pub output_index: OutputIndex,
    pub content_index: ContentPartIndex,
    pub text: String,
}

/// Reasoning, streamed as the content of a reasoning item. Addressed like any
/// other content part.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningTextDeltaEvent {
    pub item_id: ItemId,
    pub output_index: OutputIndex,
    pub content_index: ContentPartIndex,
    pub delta: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningTextDoneEvent {
    pub item_id: ItemId,
    pub output_index: OutputIndex,
    pub content_index: ContentPartIndex,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionCallArgumentsDeltaEvent {
    pub item_id: ItemId,
    pub output_index: OutputIndex,
    pub delta: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionCallArgumentsDoneEvent {
    pub item_id: ItemId,
    pub output_index: OutputIndex,
    pub arguments: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpCallArgumentsDeltaEvent {
    pub item_id: ItemId,
    pub output_index: OutputIndex,
    pub delta: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpCallArgumentsDoneEvent {
    pub item_id: ItemId,
    pub output_index: OutputIndex,
    pub arguments: String,
}

/// The events that follow an MCP item's progress name only the item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpEvent {
    pub item_id: ItemId,
    pub output_index: OutputIndex,
}

/// One Responses API event, tagged on the wire's `type`. An event is about
/// the response as a whole, one item in it, or one part of an item; a response
/// can span several generations, but no event marks where one ends.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EventKind {
    /// The response has started, with nothing in it yet. Always the first event.
    #[serde(rename = "response.created")]
    Created { response: ResponseObject },
    /// The response is being generated. Follows `Created`.
    #[serde(rename = "response.in_progress")]
    InProgress { response: ResponseObject },
    /// The whole response is done, with every item finished, and carries its
    /// usage. It's the response's last event.
    #[serde(rename = "response.completed")]
    Completed { response: ResponseObject },
    /// The whole response ended before the model ended it, for example out of
    /// tokens, as its `incomplete_details` say. Its items are still finished,
    /// and it's the response's last event.
    #[serde(rename = "response.incomplete")]
    Incomplete { response: ResponseObject },
    /// An item starts: a message, reasoning or a call.
    #[serde(rename = "response.output_item.added")]
    OutputItemAdded(OutputItemAddedEvent),
    /// An item is finished, and carries it whole.
    #[serde(rename = "response.output_item.done")]
    OutputItemDone(OutputItemDoneEvent),
    /// A content part starts in a message or in reasoning.
    #[serde(rename = "response.content_part.added")]
    ContentPartAdded(ContentPartAddedEvent),
    /// A content part is finished, and carries it whole.
    #[serde(rename = "response.content_part.done")]
    ContentPartDone(ContentPartDoneEvent),
    /// More text for a message's content part.
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta(OutputTextDeltaEvent),
    /// All of a message's content part's text.
    #[serde(rename = "response.output_text.done")]
    OutputTextDone(OutputTextDoneEvent),
    /// More text for a reasoning item's content part.
    #[serde(rename = "response.reasoning_text.delta")]
    ReasoningTextDelta(ReasoningTextDeltaEvent),
    /// All of a reasoning item's content part's text.
    #[serde(rename = "response.reasoning_text.done")]
    ReasoningTextDone(ReasoningTextDoneEvent),
    /// More of the arguments of a call the client is to run.
    #[serde(rename = "response.function_call_arguments.delta")]
    FunctionCallArgumentsDelta(FunctionCallArgumentsDeltaEvent),
    /// All the arguments of a call the client is to run.
    #[serde(rename = "response.function_call_arguments.done")]
    FunctionCallArgumentsDone(FunctionCallArgumentsDoneEvent),
    /// More of the arguments of a call the server runs.
    #[serde(rename = "response.mcp_call_arguments.delta")]
    McpCallArgumentsDelta(McpCallArgumentsDeltaEvent),
    /// All the arguments of a call the server runs.
    #[serde(rename = "response.mcp_call_arguments.done")]
    McpCallArgumentsDone(McpCallArgumentsDoneEvent),
    /// A call the server runs has started. Follows the call's `OutputItemAdded`.
    #[serde(rename = "response.mcp_call.in_progress")]
    McpCallInProgress(McpEvent),
    /// A call the server runs has returned. Its output comes with the call's
    /// `OutputItemDone`.
    #[serde(rename = "response.mcp_call.completed")]
    McpCallCompleted(McpEvent),
    /// A call the server runs has failed. Its error comes with the call's
    /// `OutputItemDone`.
    #[serde(rename = "response.mcp_call.failed")]
    McpCallFailed(McpEvent),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamEvent {
    pub sequence_number: SequenceNumber,
    #[serde(flatten)]
    pub kind: EventKind,
    /// The generated tokens the event stands for, possibly none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tokens: Vec<Token>,
}
