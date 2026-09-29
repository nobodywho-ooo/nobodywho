use serde::{Deserialize, Serialize};

use crate::output_format::Token;

use crate::event_stream::response::{
    ContentPart, ContentPartIndex, Item, ItemFields, ItemId, ResponseObject, Status, SummaryIndex,
};

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
#[serde(from = "OutputItemFields")]
pub struct OutputItemAddedEvent {
    pub output_index: OutputIndex,
    pub item: Item,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "OutputItemFields")]
pub struct OutputItemDoneEvent {
    pub output_index: OutputIndex,
    pub item: Item,
}

/// Both output-item events carry the same payload. They differ only in the state
/// they imply for an item the wire left unstatused.
#[derive(Deserialize)]
struct OutputItemFields {
    output_index: OutputIndex,
    item: ItemFields,
}

impl From<OutputItemFields> for OutputItemAddedEvent {
    fn from(fields: OutputItemFields) -> Self {
        let item = fields.item.into_item(Status::InProgress);
        debug_assert_eq!(
            item.status,
            Status::InProgress,
            "an added item has to be in progress"
        );
        OutputItemAddedEvent {
            output_index: fields.output_index,
            item,
        }
    }
}

impl From<OutputItemFields> for OutputItemDoneEvent {
    fn from(fields: OutputItemFields) -> Self {
        OutputItemDoneEvent {
            output_index: fields.output_index,
            item: fields.item.into_item(Status::Completed),
        }
    }
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

/// Reasoning streamed as the content of a reasoning item, by providers that have
/// no reasoning summaries. Addressed like any other content part.
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

/// A reasoning item's summary has its own part events, rather than the shared
/// `ContentPart*Event`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningSummaryPartAddedEvent {
    pub item_id: ItemId,
    pub output_index: OutputIndex,
    pub summary_index: SummaryIndex,
    pub part: ContentPart,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningSummaryPartDoneEvent {
    pub item_id: ItemId,
    pub output_index: OutputIndex,
    pub summary_index: SummaryIndex,
    pub part: ContentPart,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningSummaryTextDeltaEvent {
    pub item_id: ItemId,
    pub output_index: OutputIndex,
    pub summary_index: SummaryIndex,
    pub delta: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningSummaryTextDoneEvent {
    pub item_id: ItemId,
    pub output_index: OutputIndex,
    pub summary_index: SummaryIndex,
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
    /// A content part starts in a message, or in reasoning without a summary.
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
    /// A summary part starts in a reasoning item.
    #[serde(rename = "response.reasoning_summary_part.added")]
    ReasoningSummaryPartAdded(ReasoningSummaryPartAddedEvent),
    /// A summary part is finished, and carries it whole.
    #[serde(rename = "response.reasoning_summary_part.done")]
    ReasoningSummaryPartDone(ReasoningSummaryPartDoneEvent),
    /// More text for a reasoning item's summary part.
    #[serde(rename = "response.reasoning_summary_text.delta")]
    ReasoningSummaryTextDelta(ReasoningSummaryTextDeltaEvent),
    /// All of a reasoning item's summary part's text.
    #[serde(rename = "response.reasoning_summary_text.done")]
    ReasoningSummaryTextDone(ReasoningSummaryTextDoneEvent),
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
    /// The server has started listing an MCP server's tools.
    #[serde(rename = "response.mcp_list_tools.in_progress")]
    McpListToolsInProgress(McpEvent),
    /// The server has listed an MCP server's tools. They come with the item's
    /// `OutputItemDone`.
    #[serde(rename = "response.mcp_list_tools.completed")]
    McpListToolsCompleted(McpEvent),
    /// The server couldn't list an MCP server's tools.
    #[serde(rename = "response.mcp_list_tools.failed")]
    McpListToolsFailed(McpEvent),
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
