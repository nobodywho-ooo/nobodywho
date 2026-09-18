pub mod event;
pub mod response;

use std::collections::HashMap;

use crate::event_stream::{
    event::{EventKind, SequenceNumber, StreamEvent},
    response::{ResponseId, ResponseObject, Status},
};

struct EventConsumer {
    responses: HashMap<ResponseId, ResponseObject>,
    active_response_id: Option<ResponseId>,
    cur_sequence_number: SequenceNumber,
}

impl EventConsumer {
    pub fn new() -> EventConsumer {
        EventConsumer {
            responses: HashMap::new(),
            active_response_id: None,
            cur_sequence_number: SequenceNumber::start(),
        }
    }

    pub fn consume_event(&mut self, event: StreamEvent) -> Result<(), EventStreamError> {
        match event {
            StreamEvent {
                kind: EventKind::Created { response },
                sequence_number,
                ..
            } => {
                assert_eq!(
                    sequence_number,
                    SequenceNumber::start(),
                    "Created event must have sequence index 0"
                );
                if self.active_response_id.is_some() {
                    panic!(
                        "Created event received, but there is already an active response with id {:?}",
                        self.active_response_id
                    );
                }
                // Sequence indices restart with each response.
                self.cur_sequence_number = sequence_number.next();
                let response_id = response.id().clone();
                self.responses.insert(response_id.clone(), response);
                self.active_response_id = Some(response_id);
            }
            event => {
                if event.sequence_number != self.cur_sequence_number {
                    panic!(
                        "Event sequence index mismatch: expected {:?}, got {:?}",
                        self.cur_sequence_number, event.sequence_number
                    );
                }
                self.cur_sequence_number = event.sequence_number.next();

                let active_response_id = self
                    .active_response_id
                    .as_ref()
                    .expect("Event received, but there is no active response to consume it for");
                let response = self.responses.get_mut(active_response_id).expect(
                    "Active response id is set, but the response does not exist in the responses map",
                );
                response.consume_event(event)?;

                if response.status() == Status::Completed {
                    self.active_response_id = None;
                }
            }
        }
        Ok(())
    }
}

pub enum EventStreamError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Replay a recorded conversation through an `EventConsumer`.
    ///
    /// The asserts inside `consume_event` are the assertions; this only has to get
    /// every event in, in order.
    fn replay(recording: &str) {
        let events: Vec<StreamEvent> = serde_json::from_str(recording)
            .expect("every event in the recording must map onto an `EventKind`");

        let mut consumer = EventConsumer::new();
        for event in events {
            if consumer.consume_event(event).is_err() {
                panic!("EventConsumer rejected an event");
            }
        }
    }

    /// One test per recorded conversation, for the recordings in one directory.
    macro_rules! conversations {
        ($dir:literal) => {
            #[test]
            fn simple_text() {
                replay(include_str!(concat!($dir, "simple_text.json")));
            }

            #[test]
            fn multi_turn_text() {
                replay(include_str!(concat!($dir, "multi_turn_text.json")));
            }

            #[test]
            fn single_tool_call() {
                replay(include_str!(concat!($dir, "single_tool_call.json")));
            }

            #[test]
            fn parallel_tool_calls() {
                replay(include_str!(concat!($dir, "parallel_tool_calls.json")));
            }

            #[test]
            fn reasoning_with_tool_call() {
                replay(include_str!(concat!($dir, "reasoning_with_tool_call.json")));
            }

            #[test]
            fn incomplete_max_output_tokens() {
                replay(include_str!(concat!(
                    $dir,
                    "incomplete_max_output_tokens.json"
                )));
            }
        };
    }

    mod openai_gpt_5_nano {
        use super::*;
        conversations!("../../../agatest/streams/");
    }

    mod openrouter_gpt_5_nano {
        use super::*;
        conversations!("../../../agatest/streams/openrouter/gpt-5-nano/");
    }

    mod openrouter_claude_haiku_4_5 {
        use super::*;
        conversations!("../../../agatest/streams/openrouter/claude-haiku-4.5/");
    }
}
