pub mod event;
pub mod response;

use std::collections::HashMap;

use rand::Rng;

use crate::{
    event_stream::{
        event::{
            ContentPartAddedEvent, ContentPartDoneEvent, EventKind,
            FunctionCallArgumentsDeltaEvent, FunctionCallArgumentsDoneEvent, OutputIndex,
            OutputItemAddedEvent, OutputItemDoneEvent, OutputTextDeltaEvent, OutputTextDoneEvent,
            ReasoningTextDeltaEvent, ReasoningTextDoneEvent, SequenceNumber, StreamEvent,
        },
        response::{
            ContentPart, ContentPartIndex, FunctionCallItem, Item, ItemId, ItemKind, MessageItem,
            ReasoningItem, ResponseId, ResponseObject, ResponseUsage, Role, Status,
        },
    },
    output_format::{self, Piece, PieceKind, Token},
};

/// Turns the pieces of a model's response into the events of a Responses API
/// stream, one piece at a time.
pub struct EventStream {
    /// The response as the events so far describe it.
    response: ResponseObject,
    next_sequence_number: SequenceNumber,
    open: Option<OpenItem>,
    /// Tokens of pieces that make no event, which go on the next event.
    carried: Vec<Token>,
    input_tokens: u64,
    output_tokens: u64,
}

struct OpenItem {
    index: OutputIndex,
    id: ItemId,
    kind: Open,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Open {
    Text,
    Thinking,
    ToolCall,
}

impl EventStream {
    /// Starts a response to a prompt of `input_tokens` tokens, along with the
    /// events that announce it.
    pub fn new(input_tokens: u64, rng: &mut impl Rng) -> (Self, Vec<StreamEvent>) {
        let id = ResponseId::generate(rng);
        let mut stream = EventStream {
            response: ResponseObject::init(id),
            next_sequence_number: SequenceNumber::start(),
            open: None,
            carried: Vec::new(),
            input_tokens,
            output_tokens: 0,
        };
        let response = stream.response.clone();
        let events = vec![
            stream.emit(
                EventKind::Created {
                    response: response.clone(),
                },
                vec![],
            ),
            stream.emit(EventKind::InProgress { response }, vec![]),
        ];
        (stream, events)
    }

    /// The response as the events so far describe it.
    pub fn response(&self) -> &ResponseObject {
        &self.response
    }

    /// Takes the next piece of the response and returns the events it makes.
    /// Each event carries the tokens of the piece it came from, with the tags
    /// on the events that add and finish an item. Warnings make no events, so
    /// a caller that wants to log them should do so before passing them on.
    pub fn consume_piece(
        &mut self,
        piece: Piece,
        rng: &mut impl Rng,
    ) -> Result<Vec<StreamEvent>, EventStreamError> {
        if self.response.status() != Status::InProgress {
            return Err(EventStreamError::Ended);
        }
        let Piece { kind, tokens } = piece;
        let fits = match kind {
            PieceKind::Open(_) | PieceKind::End { .. } => self.open.is_none(),
            PieceKind::Delta(_) | PieceKind::Close => self.open.is_some(),
            PieceKind::Warning(_) => true,
        };
        if !fits {
            return Err(EventStreamError::Misplaced(kind));
        }
        self.output_tokens += tokens.len() as u64;

        let events = match kind {
            PieceKind::Open(item) => self.open_item(item, tokens, rng),
            PieceKind::Delta(delta) => vec![self.delta(delta, tokens)],
            PieceKind::Close => self.close_item(tokens),
            PieceKind::Warning(_) => {
                self.carried.extend(tokens);
                vec![]
            }
            PieceKind::End { cut_off } => {
                let usage = ResponseUsage {
                    input_tokens: self.input_tokens,
                    output_tokens: self.output_tokens,
                };
                let kind = if cut_off {
                    EventKind::Incomplete {
                        response: self.ended(Status::Incomplete, usage),
                    }
                } else {
                    EventKind::Completed {
                        response: self.ended(Status::Completed, usage),
                    }
                };
                vec![self.emit(kind, tokens)]
            }
        };
        Ok(events)
    }

    /// Numbers an event, gives it the tokens carried so far along with its
    /// own, and applies it to the response.
    fn emit(&mut self, kind: EventKind, tokens: Vec<Token>) -> StreamEvent {
        let mut all_tokens = std::mem::take(&mut self.carried);
        all_tokens.extend(tokens);
        let event = StreamEvent {
            sequence_number: self.next_sequence_number,
            kind,
            tokens: all_tokens,
        };
        self.next_sequence_number = self.next_sequence_number.next();
        // The response starts out as the one `Created` carries.
        if !matches!(event.kind, EventKind::Created { .. }) {
            self.response.consume_event(event.clone());
        }
        event
    }

    fn open_item(
        &mut self,
        item: output_format::Item,
        tokens: Vec<Token>,
        rng: &mut impl Rng,
    ) -> Vec<StreamEvent> {
        let index = self.next_output_index();
        let (id, kind, part, open) = match item {
            output_format::Item::Text => (
                ItemId::generate_message(rng),
                ItemKind::Message(MessageItem {
                    role: Role::Assistant,
                    content: vec![],
                }),
                Some(ContentPart::output(String::new())),
                Open::Text,
            ),
            output_format::Item::Thinking => (
                ItemId::generate_reasoning(rng),
                ItemKind::Reasoning(ReasoningItem {
                    summary: vec![],
                    content: vec![],
                }),
                Some(ContentPart::reasoning(String::new())),
                Open::Thinking,
            ),
            output_format::Item::ToolCall { name } => (
                ItemId::generate_function_call(rng),
                ItemKind::FunctionCall(FunctionCallItem::new(name, rng)),
                None,
                Open::ToolCall,
            ),
        };
        let item = Item {
            id: id.clone(),
            status: Status::InProgress,
            kind,
        };
        let mut events = vec![self.emit(
            EventKind::OutputItemAdded(OutputItemAddedEvent {
                output_index: index,
                item,
            }),
            tokens,
        )];
        if let Some(part) = part {
            events.push(self.emit(
                EventKind::ContentPartAdded(ContentPartAddedEvent {
                    item_id: id.clone(),
                    output_index: index,
                    content_index: ContentPartIndex(0),
                    part,
                }),
                vec![],
            ));
        }
        self.open = Some(OpenItem {
            index,
            id,
            kind: open,
        });
        events
    }

    fn delta(&mut self, delta: String, tokens: Vec<Token>) -> StreamEvent {
        let open = self.open.as_ref().expect("checked by the caller");
        let item_id = open.id.clone();
        let output_index = open.index;
        let kind = match open.kind {
            Open::Text => EventKind::OutputTextDelta(OutputTextDeltaEvent {
                item_id,
                output_index,
                content_index: ContentPartIndex(0),
                delta,
            }),
            Open::Thinking => EventKind::ReasoningTextDelta(ReasoningTextDeltaEvent {
                item_id,
                output_index,
                content_index: ContentPartIndex(0),
                delta,
            }),
            Open::ToolCall => {
                EventKind::FunctionCallArgumentsDelta(FunctionCallArgumentsDeltaEvent {
                    item_id,
                    output_index,
                    delta,
                })
            }
        };
        self.emit(kind, tokens)
    }

    fn close_item(&mut self, tokens: Vec<Token>) -> Vec<StreamEvent> {
        let open = self.open.take().expect("checked by the caller");
        let item = self.done_item(open.index);
        let mut events = match &item.kind {
            ItemKind::Message(MessageItem { content, .. }) => {
                let text = content[0].text.clone();
                vec![
                    self.emit(
                        EventKind::OutputTextDone(OutputTextDoneEvent {
                            item_id: open.id.clone(),
                            output_index: open.index,
                            content_index: ContentPartIndex(0),
                            text: text.clone(),
                        }),
                        vec![],
                    ),
                    self.part_done(&open, ContentPart::output(text)),
                ]
            }
            ItemKind::Reasoning(ReasoningItem { content, .. }) => {
                let text = content[0].text.clone();
                vec![
                    self.emit(
                        EventKind::ReasoningTextDone(ReasoningTextDoneEvent {
                            item_id: open.id.clone(),
                            output_index: open.index,
                            content_index: ContentPartIndex(0),
                            text: text.clone(),
                        }),
                        vec![],
                    ),
                    self.part_done(&open, ContentPart::reasoning(text)),
                ]
            }
            ItemKind::FunctionCall(FunctionCallItem { arguments, .. }) => {
                vec![self.emit(
                    EventKind::FunctionCallArgumentsDone(FunctionCallArgumentsDoneEvent {
                        item_id: open.id.clone(),
                        output_index: open.index,
                        arguments: arguments.clone(),
                    }),
                    vec![],
                )]
            }
            ItemKind::FunctionCallOutput(_) => unreachable!("a model doesn't write these"),
        };
        events.push(self.emit(
            EventKind::OutputItemDone(OutputItemDoneEvent {
                output_index: open.index,
                item,
            }),
            tokens,
        ));
        events
    }

    fn part_done(&mut self, open: &OpenItem, part: ContentPart) -> StreamEvent {
        self.emit(
            EventKind::ContentPartDone(ContentPartDoneEvent {
                item_id: open.id.clone(),
                output_index: open.index,
                content_index: ContentPartIndex(0),
                part,
            }),
            vec![],
        )
    }

    fn next_output_index(&self) -> OutputIndex {
        OutputIndex(self.response.output().len())
    }

    /// An item as it is once done.
    fn done_item(&self, index: OutputIndex) -> Item {
        let mut item = self.response.output()[index.0].clone();
        item.status = Status::Completed;
        item
    }

    /// The response as it is once ended.
    fn ended(&self, status: Status, usage: ResponseUsage) -> ResponseObject {
        let mut response = self.response.clone();
        response.status = status;
        response.usage = Some(usage);
        response
    }
}

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
                response.consume_event(event);

                if response.status() == Status::Completed {
                    self.active_response_id = None;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EventStreamError {
    #[error("the response has already ended")]
    Ended,
    #[error("a piece doesn't fit where the response is: {0:?}")]
    Misplaced(PieceKind),
}

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

    // ========================================================================
    // Streams made from our own generations
    // ========================================================================

    use crate::output_format::{Qwen3, ResolvedFormat};
    use crate::tool_calling::Tool;
    use llama_cpp_2::model::{params::LlamaModelParams, AddBos, LlamaModel};
    use rand::{rngs::StdRng, SeedableRng};
    use serde_json::{json, Value};
    use std::sync::Arc;

    fn qwen3_vocab() -> LlamaModel {
        let path = std::env::var("TEST_MODEL").unwrap_or_else(|_| "model.gguf".to_string());
        let params = LlamaModelParams::default().with_vocab_only(true);
        LlamaModel::load_from_file(&crate::llm::LLAMA_BACKEND, &path, &params)
            .unwrap_or_else(|e| panic!("failed to load vocabulary from {path}: {e}"))
    }

    /// The pieces the splitter makes of `response`, a token at a time as the
    /// model would write it after `prompt`.
    fn pieces(prompt: &str, response: &str) -> Vec<Piece> {
        let model = qwen3_vocab();
        let format = ResolvedFormat::new(&Qwen3, &model).unwrap();
        let tools = [Tool::new(
            "get_weather",
            "",
            json!({
                "type": "object",
                "properties": { "city": { "type": "string" } },
                "required": ["city"],
            }),
            Arc::new(|_| String::new()),
        )];
        let mut splitter = format.splitter(&tools, prompt);
        let mut pieces = Vec::new();
        for token in model.str_to_token(response, AddBos::Never).unwrap() {
            let bytes = model.token_to_piece_bytes(token, 64, true, None).unwrap();
            pieces.extend(splitter.push(token, &bytes));
        }
        pieces.extend(splitter.finish());
        pieces
    }

    const INPUT_TOKENS: u64 = 12;

    /// The events for `pieces`, which carry the same tokens in the same order.
    fn stream(pieces: Vec<Piece>) -> Vec<StreamEvent> {
        let tokens: Vec<Token> = pieces.iter().flat_map(|p| p.tokens.clone()).collect();
        let mut rng = StdRng::seed_from_u64(0);
        let (mut stream, mut events) = EventStream::new(INPUT_TOKENS, &mut rng);
        for piece in pieces {
            events.extend(stream.consume_piece(piece, &mut rng).unwrap());
        }
        let event_tokens: Vec<Token> = events.iter().flat_map(|e| e.tokens.clone()).collect();
        assert_eq!(event_tokens, tokens);
        events
    }

    /// The text of the tokens on the events of `event_type`.
    fn tokens_of(events: &[StreamEvent], event_type: &str) -> Vec<String> {
        events
            .iter()
            .filter(|event| serde_json::to_value(event).unwrap()["type"] == event_type)
            .map(|event| event.tokens.iter().map(|t| t.text.as_str()).collect())
            .collect()
    }

    /// The response a client builds from the events, sent to it as JSON.
    fn consume(events: &[StreamEvent]) -> ResponseObject {
        let json = serde_json::to_string(events).unwrap();
        let events: Vec<StreamEvent> = serde_json::from_str(&json).unwrap();
        let mut consumer = EventConsumer::new();
        for event in events {
            consumer.consume_event(event).unwrap();
        }
        assert_eq!(consumer.responses.len(), 1);
        consumer.responses.into_values().next().unwrap()
    }

    /// Each item's type and text, or a call's name and arguments.
    fn output(response: &ResponseObject) -> Vec<(&'static str, String)> {
        response
            .output()
            .iter()
            .map(|item| {
                assert_eq!(item.status, Status::Completed);
                match &item.kind {
                    ItemKind::Reasoning(ReasoningItem { content, .. }) => {
                        ("reasoning", content[0].text.clone())
                    }
                    ItemKind::Message(MessageItem { content, .. }) => {
                        ("message", content[0].text.clone())
                    }
                    ItemKind::FunctionCall(FunctionCallItem {
                        name, arguments, ..
                    }) => ("function_call", format!("{name}{arguments}")),
                    ItemKind::FunctionCallOutput(_) => unreachable!("a model doesn't write these"),
                }
            })
            .collect()
    }

    /// The events' types, with runs of one type as one.
    fn types(events: &[StreamEvent]) -> Vec<String> {
        let mut types: Vec<String> = events
            .iter()
            .map(|event| {
                serde_json::to_value(event).unwrap()["type"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        types.dedup();
        types
    }

    const PROMPT: &str = "<|im_start|>user\nHi<|im_end|>\n<|im_start|>assistant\n";

    #[test]
    fn an_answer_streams_as_one_message() {
        let events = stream(pieces(PROMPT, "Hello there, how are you?<|im_end|>"));
        let response = consume(&events);
        assert_eq!(response.status(), Status::Completed);
        assert_eq!(
            output(&response),
            [("message", "Hello there, how are you?".to_string())]
        );
        let deltas = events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::OutputTextDelta(_)))
            .count();
        assert!(deltas > 1, "the text should stream, not arrive whole");
    }

    /// The same events, in the same order, as Claude's reasoning and calls.
    #[test]
    fn reasoning_and_calls_stream_as_providers_do() {
        let call = |city| {
            format!(
                "<tool_call>\n{{\"name\": \"get_weather\", \"arguments\": {{\"city\": \"{city}\"}}}}\n</tool_call>"
            )
        };
        let response = format!(
            "<think>\nI should check both.\n</think>\n\n{}\n{}<|im_end|>",
            call("Copenhagen"),
            call("Oslo")
        );
        let events = stream(pieces(PROMPT, &response));
        assert_eq!(
            output(&consume(&events)),
            [
                ("reasoning", "I should check both.".to_string()),
                (
                    "function_call",
                    r#"get_weather{"city":"Copenhagen"}"#.to_string()
                ),
                ("function_call", r#"get_weather{"city":"Oslo"}"#.to_string()),
            ]
        );

        let recording: Vec<Value> = serde_json::from_str(include_str!(
            "../../../agatest/streams/openrouter/claude-haiku-4.5/reasoning_with_tool_call.json"
        ))
        .unwrap();
        // The recording's first response, which reasons and calls.
        let end = recording
            .iter()
            .position(|e| e["type"] == "response.completed")
            .unwrap();
        let mut expected: Vec<String> = recording[..=end]
            .iter()
            .map(|e| e["type"].as_str().unwrap().to_string())
            .collect();
        expected.dedup();
        assert_eq!(types(&events), expected);
    }

    #[test]
    fn text_before_a_call_is_a_message() {
        let events = stream(pieces(
            PROMPT,
            "Let me look that up.\n<tool_call>\n{\"name\": \"get_weather\", \"arguments\": {\"city\": \"Oslo\"}}\n</tool_call><|im_end|>",
        ));
        assert_eq!(
            output(&consume(&events)),
            [
                ("message", "Let me look that up.".to_string()),
                ("function_call", r#"get_weather{"city":"Oslo"}"#.to_string()),
            ]
        );
    }

    #[test]
    fn a_prompt_can_open_the_reasoning() {
        let prompt = format!("{PROMPT}<think>\n");
        let events = stream(pieces(&prompt, "Easy.\n</think>\n\nIt's 4.<|im_end|>"));
        assert_eq!(
            output(&consume(&events)),
            [
                ("reasoning", "Easy.".to_string()),
                ("message", "It's 4.".to_string())
            ]
        );
    }

    #[test]
    fn empty_reasoning_is_an_empty_item() {
        let events = stream(pieces(PROMPT, "<think>\n\n</think>\n\nHi!<|im_end|>"));
        assert_eq!(
            output(&consume(&events)),
            [("reasoning", String::new()), ("message", "Hi!".to_string())]
        );
    }

    /// Markers go on the events that add and finish their items, and the end
    /// of generation on the event that ends the response.
    #[test]
    fn tags_go_on_the_events_that_add_and_finish_items() {
        let events = stream(pieces(
            PROMPT,
            "<think>\nHmm.\n</think>\n\n<tool_call>\n{\"name\": \"get_weather\", \"arguments\": {\"city\": \"Oslo\"}}\n</tool_call><|im_end|>",
        ));
        let added = tokens_of(&events, "response.output_item.added");
        assert!(added[0].starts_with("<think>"), "{added:?}");
        assert!(added[1].starts_with("<tool_call>"), "{added:?}");
        let done = tokens_of(&events, "response.output_item.done");
        assert!(done[0].contains("</think>"), "{done:?}");
        assert!(done[1].contains("</tool_call>"), "{done:?}");
        assert_eq!(tokens_of(&events, "response.completed"), ["<|im_end|>"]);
    }

    #[test]
    fn usage_counts_every_token() {
        let response = "<think>\nHmm.\n</think>\n\nHi!<|im_end|>";
        let generated = qwen3_vocab()
            .str_to_token(response, AddBos::Never)
            .unwrap()
            .len() as u64;
        let usage = consume(&stream(pieces(PROMPT, response))).usage.unwrap();
        assert_eq!(usage.input_tokens, INPUT_TOKENS);
        assert_eq!(usage.output_tokens, generated);
    }

    #[test]
    fn a_cut_off_response_is_incomplete() {
        let events = stream(pieces(PROMPT, "<think>\nThinking about it"));
        let response = consume(&events);
        assert_eq!(response.status(), Status::Incomplete);
        assert_eq!(
            output(&response),
            [("reasoning", "Thinking about it".to_string())]
        );
        assert_eq!(
            types(&events).last().map(String::as_str),
            Some("response.incomplete")
        );
    }

    #[test]
    fn an_unreadable_call_is_shown_as_text() {
        let events = stream(pieces(
            PROMPT,
            "<tool_call>\nnot json\n</tool_call><|im_end|>",
        ));
        assert_eq!(
            output(&consume(&events)),
            [("message", "<tool_call>\nnot json\n</tool_call>".to_string())]
        );
    }

    fn piece(kind: PieceKind) -> Piece {
        Piece {
            kind,
            tokens: vec![],
        }
    }

    #[test]
    fn pieces_have_to_fit_where_the_response_is() {
        let mut rng = StdRng::seed_from_u64(0);
        let (mut stream, _) = EventStream::new(INPUT_TOKENS, &mut rng);
        let delta = PieceKind::Delta("more".to_string());
        assert!(matches!(
            stream.consume_piece(piece(delta.clone()), &mut rng),
            Err(EventStreamError::Misplaced(_))
        ));
        let end = PieceKind::End { cut_off: false };
        stream.consume_piece(piece(end), &mut rng).unwrap();
        assert!(matches!(
            stream.consume_piece(piece(delta), &mut rng),
            Err(EventStreamError::Ended)
        ));
    }
}
