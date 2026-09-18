use serde::{Deserialize, Serialize};

use crate::event_stream::response::{
    ContentPart, ContentPartIndex, Item, ItemFields, ItemId, ResponseObject, Status, SummaryIndex,
};

/// The item's index within the response.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OutputIndex(pub usize);

impl OutputIndex {
    pub fn next(&self) -> Self {
        OutputIndex(self.0 + 1)
    }
}

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
        OutputItemAddedEvent {
            output_index: fields.output_index,
            item: fields.item.into_item(Status::InProgress),
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

/// One Responses API event, tagged on the wire's `type`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EventKind {
    #[serde(rename = "response.created")]
    Created { response: ResponseObject },
    #[serde(rename = "response.in_progress")]
    InProgress { response: ResponseObject },
    #[serde(rename = "response.completed")]
    Completed { response: ResponseObject },
    #[serde(rename = "response.incomplete")]
    Incomplete { response: ResponseObject },
    #[serde(rename = "response.output_item.added")]
    OutputItemAdded(OutputItemAddedEvent),
    #[serde(rename = "response.output_item.done")]
    OutputItemDone(OutputItemDoneEvent),
    #[serde(rename = "response.content_part.added")]
    ContentPartAdded(ContentPartAddedEvent),
    #[serde(rename = "response.content_part.done")]
    ContentPartDone(ContentPartDoneEvent),
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta(OutputTextDeltaEvent),
    #[serde(rename = "response.output_text.done")]
    OutputTextDone(OutputTextDoneEvent),
    #[serde(rename = "response.reasoning_text.delta")]
    ReasoningTextDelta(ReasoningTextDeltaEvent),
    #[serde(rename = "response.reasoning_text.done")]
    ReasoningTextDone(ReasoningTextDoneEvent),
    #[serde(rename = "response.reasoning_summary_part.added")]
    ReasoningSummaryPartAdded(ReasoningSummaryPartAddedEvent),
    #[serde(rename = "response.reasoning_summary_part.done")]
    ReasoningSummaryPartDone(ReasoningSummaryPartDoneEvent),
    #[serde(rename = "response.reasoning_summary_text.delta")]
    ReasoningSummaryTextDelta(ReasoningSummaryTextDeltaEvent),
    #[serde(rename = "response.reasoning_summary_text.done")]
    ReasoningSummaryTextDone(ReasoningSummaryTextDoneEvent),
    #[serde(rename = "response.function_call_arguments.delta")]
    FunctionCallArgumentsDelta(FunctionCallArgumentsDeltaEvent),
    #[serde(rename = "response.function_call_arguments.done")]
    FunctionCallArgumentsDone(FunctionCallArgumentsDoneEvent),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamEvent {
    pub sequence_number: SequenceNumber,
    #[serde(flatten)]
    pub kind: EventKind,
}
