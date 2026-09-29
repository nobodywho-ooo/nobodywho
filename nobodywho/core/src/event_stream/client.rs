//! A client's view of the stream, for tests: the response rebuilt from the
//! events alone, which checks that each event fits the response so far.

use crate::event_stream::event::{
    ContentPartAddedEvent, ContentPartDoneEvent, EventKind, FunctionCallArgumentsDeltaEvent,
    FunctionCallArgumentsDoneEvent, McpCallArgumentsDeltaEvent, McpCallArgumentsDoneEvent,
    McpEvent, OutputIndex, OutputItemAddedEvent, OutputItemDoneEvent, OutputTextDeltaEvent,
    OutputTextDoneEvent, ReasoningTextDeltaEvent, ReasoningTextDoneEvent, StreamEvent,
};
use crate::event_stream::response::{
    FunctionCallItem, Item, ItemId, ItemKind, ItemType, McpCallItem, MessageItem, ReasoningItem,
    ResponseObject, Status,
};

impl ItemKind {
    fn item_type(&self) -> ItemType {
        match self {
            ItemKind::Reasoning { .. } => ItemType::Reasoning,
            ItemKind::FunctionCall { .. } => ItemType::FunctionCall,
            ItemKind::Message { .. } => ItemType::Message,
            ItemKind::McpCall { .. } => ItemType::McpCall,
        }
    }
}

impl ResponseObject {
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
