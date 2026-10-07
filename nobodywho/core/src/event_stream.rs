pub mod event;
pub mod response;

use rand::rngs::StdRng;

use crate::{
    event_stream::{
        event::{
            ContentPartAddedEvent, ContentPartDoneEvent, EventKind,
            FunctionCallArgumentsDeltaEvent, FunctionCallArgumentsDoneEvent,
            McpCallArgumentsDeltaEvent, McpCallArgumentsDoneEvent, McpEvent, OutputIndex,
            OutputItemAddedEvent, OutputItemDoneEvent, OutputTextDeltaEvent, OutputTextDoneEvent,
            ReasoningTextDeltaEvent, ReasoningTextDoneEvent, SequenceNumber, StreamEvent,
        },
        response::{
            ContentPart, ContentPartIndex, FunctionCallItem, IncompleteDetails, Item, ItemId,
            ItemKind, McpCallError, McpCallItem, MessageItem, ReasoningItem, ResponseId,
            ResponseObject, ResponseUsage, Role, Status,
        },
    },
    output_format::{self, Piece, PieceKind, Token},
};

/// The label our own tools go by as MCP calls, as they're on no MCP server.
const SERVER_LABEL: &str = "nobodywho";

/// Turns the pieces of a model's generations into the events of one Response's
/// API stream, one piece at a time. A response can span several generations,
/// including the calls we run between them.
pub struct EventStream {
    /// The response as the events so far describe it.
    response: ResponseObject,
    next_sequence_number: SequenceNumber,
    open: Option<OpenItem>,
    /// Tokens of pieces that make no event, which go on the next event.
    carried: Vec<Token>,
    input_tokens: u64,
    output_tokens: u64,
    /// Whether the calls the model makes are ours to run, as MCP calls, rather
    /// than the client's, as function calls.
    runs_calls: bool,
    /// Makes the response's ids, so the same `rng` and pieces give the same
    /// events.
    rng: StdRng,
}

/// The item an `EventStream` is writing, which the pieces up to its close go
/// to. Only the `EventStream` uses it.
struct OpenItem {
    index: OutputIndex,
    id: ItemId,
    kind: Open,
}

/// The kind of item an `EventStream` is writing, which decides the events its
/// deltas and close make. Only the `EventStream` uses it; the item itself is in
/// the response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Open {
    Text,
    Thinking,
    FunctionCall,
    McpCall,
}

impl EventStream {
    /// Starts a response, along with the events that announce it. `rng` makes
    /// its ids, so a seeded one makes the stream deterministic.
    pub fn new(runs_calls: bool, mut rng: StdRng) -> (Self, Vec<StreamEvent>) {
        let id = ResponseId::generate(&mut rng);
        let mut stream = EventStream {
            response: ResponseObject::init(id),
            next_sequence_number: SequenceNumber::start(),
            open: None,
            carried: Vec::new(),
            input_tokens: 0,
            output_tokens: 0,
            runs_calls,
            rng,
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

    /// How many tokens the model has generated in the response so far.
    pub fn output_tokens(&self) -> u64 {
        self.output_tokens
    }

    /// Adds `tokens` to the response's input tokens. Call it with the size of
    /// each generation's prompt, as the model reads all of it.
    pub fn add_input_tokens(&mut self, tokens: u64) {
        self.input_tokens += tokens;
    }

    /// Takes the next piece of a generation and returns the events it makes.
    /// Each event carries the tokens of the piece it came from, with the tags
    /// on the events that add and finish an item. The end of a generation
    /// makes no events, as the response goes on until
    /// [`complete`](Self::complete) or [`cut_off`](Self::cut_off).
    pub fn consume_piece(&mut self, piece: Piece) -> Result<Vec<StreamEvent>, EventStreamError> {
        if self.response.status != Status::InProgress {
            return Err(EventStreamError::Ended);
        }
        let Piece { kind, tokens } = piece;
        let fits = match kind {
            PieceKind::Open(_) | PieceKind::End { .. } => self.open.is_none(),
            PieceKind::Delta(_) | PieceKind::Close => self.open.is_some(),
        };
        if !fits {
            return Err(EventStreamError::Misplaced(kind));
        }
        self.output_tokens += tokens.len() as u64;

        Ok(match kind {
            PieceKind::Open(item) => self.open_item(item, tokens),
            PieceKind::Delta(delta) => vec![self.delta(delta, tokens)],
            PieceKind::Close => self.close_item(tokens),
            PieceKind::End { .. } => {
                self.carried.extend(tokens);
                vec![]
            }
        })
    }

    /// Finishes the call at `index`, which we ran, with what it returned.
    pub fn call_result(
        &mut self,
        index: OutputIndex,
        result: Result<String, McpCallError>,
    ) -> Result<Vec<StreamEvent>, EventStreamError> {
        let running = self.response.output.get(index.0).filter(|item| {
            item.status == Status::InProgress && matches!(item.kind, ItemKind::McpCall(_))
        });
        let Some(item) = running else {
            return Err(EventStreamError::NotRunning(index));
        };
        let mut item = item.clone();

        let ItemKind::McpCall(call) = &mut item.kind else {
            unreachable!("checked above");
        };
        let progress = McpEvent {
            item_id: item.id.clone(),
            output_index: index,
        };
        let finished = if result.is_ok() {
            item.status = Status::Completed;
            EventKind::McpCallCompleted(progress)
        } else {
            item.status = Status::Failed;
            EventKind::McpCallFailed(progress)
        };
        call.result = Some(result);
        Ok(vec![
            self.emit(finished, vec![]),
            self.emit(
                EventKind::OutputItemDone(OutputItemDoneEvent {
                    output_index: index,
                    item,
                }),
                vec![],
            ),
        ])
    }

    /// Ends the response as complete.
    pub fn complete(&mut self) -> Result<Vec<StreamEvent>, EventStreamError> {
        self.end(Status::Completed, None)
    }

    /// Ends the response before the model did, for the reason in `details`
    /// if the API has a name for it.
    pub fn cut_off(
        &mut self,
        details: Option<IncompleteDetails>,
    ) -> Result<Vec<StreamEvent>, EventStreamError> {
        self.end(Status::Incomplete, details)
    }

    fn end(
        &mut self,
        status: Status,
        incomplete_details: Option<IncompleteDetails>,
    ) -> Result<Vec<StreamEvent>, EventStreamError> {
        if self.response.status != Status::InProgress {
            return Err(EventStreamError::Ended);
        }
        let unfinished = self
            .response
            .output
            .iter()
            .any(|item| item.status == Status::InProgress);
        if self.open.is_some() || unfinished {
            return Err(EventStreamError::Unfinished);
        }
        let mut response = self.response.clone();
        response.status = status;
        response.incomplete_details = incomplete_details;
        response.usage = Some(ResponseUsage {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
        });
        let kind = match status {
            Status::Incomplete => EventKind::Incomplete { response },
            _ => EventKind::Completed { response },
        };
        Ok(vec![self.emit(kind, vec![])])
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

    fn open_item(&mut self, item: output_format::Item, tokens: Vec<Token>) -> Vec<StreamEvent> {
        let index = self.next_output_index();
        let rng = &mut self.rng;
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
            output_format::Item::ToolCall { name } if self.runs_calls => (
                ItemId::generate_mcp_call(rng),
                ItemKind::McpCall(McpCallItem {
                    server_label: SERVER_LABEL.to_string(),
                    name,
                    arguments: String::new(),
                    result: None,
                }),
                None,
                Open::McpCall,
            ),
            output_format::Item::ToolCall { name } => (
                ItemId::generate_function_call(rng),
                ItemKind::FunctionCall(FunctionCallItem::new(name, rng)),
                None,
                Open::FunctionCall,
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
        if open == Open::McpCall {
            events.push(self.emit(
                EventKind::McpCallInProgress(McpEvent {
                    item_id: id.clone(),
                    output_index: index,
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
            Open::FunctionCall => {
                EventKind::FunctionCallArgumentsDelta(FunctionCallArgumentsDeltaEvent {
                    item_id,
                    output_index,
                    delta,
                })
            }
            Open::McpCall => EventKind::McpCallArgumentsDelta(McpCallArgumentsDeltaEvent {
                item_id,
                output_index,
                delta,
            }),
        };
        self.emit(kind, tokens)
    }

    /// Finishes writing the open item. A call we run is only done once it has
    /// run, so its end tag goes on the event that finishes its arguments.
    fn close_item(&mut self, tokens: Vec<Token>) -> Vec<StreamEvent> {
        let open = self.open.take().expect("checked by the caller");
        let mut item = self.response.output[open.index.0].clone();
        item.status = Status::Completed;
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
            ItemKind::McpCall(McpCallItem { arguments, .. }) => {
                return vec![self.emit(
                    EventKind::McpCallArgumentsDone(McpCallArgumentsDoneEvent {
                        item_id: open.id.clone(),
                        output_index: open.index,
                        arguments: arguments.clone(),
                    }),
                    tokens,
                )];
            }
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
        OutputIndex(self.response.output.len())
    }
}

/// Picks out the tokens of one generation's events that stream as its text:
/// everything the model writes up to its first call, but for the end of
/// generation.
#[derive(Default)]
pub struct GenerationText {
    calls_begun: bool,
}

impl GenerationText {
    /// Takes the generation's next event, and returns the texts of its tokens
    /// that are the generation's text.
    pub fn push<'e>(&mut self, event: &'e StreamEvent) -> Vec<&'e str> {
        match &event.kind {
            EventKind::OutputItemAdded(OutputItemAddedEvent { item, .. })
                if matches!(item.kind, ItemKind::FunctionCall(_) | ItemKind::McpCall(_)) =>
            {
                self.calls_begun = true
            }
            // All they can carry is the end of generation.
            EventKind::Completed { .. } | EventKind::Incomplete { .. } => return vec![],
            _ => {}
        }
        if self.calls_begun {
            return vec![];
        }
        event.tokens.iter().map(|t| t.text.as_str()).collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EventStreamError {
    #[error("the response has already ended")]
    Ended,
    #[error("a piece doesn't fit where the response is: {0:?}")]
    Misplaced(PieceKind),
    #[error("there's no running call at output index {}", .0.0)]
    NotRunning(OutputIndex),
    #[error("the response can't end with an item unfinished")]
    Unfinished,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_stream::response::IncompleteReason;
    use crate::output_format::{qwen3, ResolvedFormat};
    use crate::tool_calling::Tool;
    use llama_cpp_2::model::{params::LlamaModelParams, LlamaModel};
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
        let format = ResolvedFormat::new(qwen3(), &model).unwrap();
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
        let mut splitter = format.splitter(tools.to_vec(), prompt);
        let mut pieces = Vec::new();
        for token in model.vocab().tokenize(response.as_bytes(), false, true) {
            let bytes = model.vocab().token_to_piece(token, true, None);
            pieces.extend(splitter.push(token, &bytes).0);
        }
        pieces.extend(splitter.finish().0);
        pieces
    }

    const INPUT_TOKENS: u64 = 12;

    /// The events of a response of one generation, `pieces`, which is cut off
    /// as out of tokens if the model didn't end it.
    fn stream(pieces: Vec<Piece>) -> Vec<StreamEvent> {
        let cut_off = pieces
            .iter()
            .any(|piece| matches!(piece.kind, PieceKind::End { cut_off: true }));
        let (mut stream, mut events) = EventStream::new(false, StdRng::seed_from_u64(0));
        events.extend(generation(&mut stream, pieces));
        let end = if cut_off {
            stream.cut_off(Some(IncompleteDetails {
                reason: IncompleteReason::MaxOutputTokens,
            }))
        } else {
            stream.complete()
        };
        events.extend(end.unwrap());
        events
    }

    /// The events of a generation, `pieces`, which carry the same tokens in
    /// the same order once the next event has taken what they carry over.
    fn generation(stream: &mut EventStream, pieces: Vec<Piece>) -> Vec<StreamEvent> {
        let tokens: Vec<Token> = pieces.iter().flat_map(|p| p.tokens.clone()).collect();
        stream.add_input_tokens(INPUT_TOKENS);
        let mut events = Vec::new();
        for piece in pieces {
            events.extend(stream.consume_piece(piece).unwrap());
        }
        let event_tokens: Vec<Token> = events.iter().flat_map(|e| e.tokens.clone()).collect();
        assert_eq!(event_tokens, tokens[..event_tokens.len()]);
        assert_eq!(stream.carried, tokens[event_tokens.len()..]);
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
        let mut events = events.into_iter();
        let created = events.next().unwrap();
        assert_eq!(created.sequence_number, SequenceNumber::start());
        let EventKind::Created { mut response } = created.kind else {
            panic!("a response starts with `Created`, not {:?}", created.kind);
        };
        for (number, event) in (1..).zip(events) {
            assert_eq!(event.sequence_number, SequenceNumber(number));
            response.consume_event(event);
        }
        response
    }

    /// Each item's type and text, or a call's name and arguments.
    fn output(response: &ResponseObject) -> Vec<(&'static str, String)> {
        response
            .output
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
                    ItemKind::McpCall(McpCallItem {
                        name, arguments, ..
                    }) => ("mcp_call", format!("{name}{arguments}")),
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
        assert_eq!(response.status, Status::Completed);
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

    /// Reasoning and calls stream as the Responses API streams them.
    #[test]
    fn reasoning_and_calls_stream_as_the_api_does() {
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

        // With repeated deltas as one.
        assert_eq!(
            types(&events),
            [
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.content_part.added",
                "response.reasoning_text.delta",
                "response.reasoning_text.done",
                "response.content_part.done",
                "response.output_item.done",
                "response.output_item.added",
                "response.function_call_arguments.delta",
                "response.function_call_arguments.done",
                "response.output_item.done",
                "response.output_item.added",
                "response.function_call_arguments.delta",
                "response.function_call_arguments.done",
                "response.output_item.done",
                "response.completed",
            ]
        );
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
            .vocab()
            .tokenize(response.as_bytes(), false, true)
            .len() as u64;
        let usage = consume(&stream(pieces(PROMPT, response))).usage.unwrap();
        assert_eq!(usage.input_tokens, INPUT_TOKENS);
        assert_eq!(usage.output_tokens, generated);
    }

    #[test]
    fn a_cut_off_response_is_incomplete() {
        let events = stream(pieces(PROMPT, "<think>\nThinking about it"));
        let response = consume(&events);
        assert_eq!(response.status, Status::Incomplete);
        assert_eq!(
            response.incomplete_details,
            Some(IncompleteDetails {
                reason: IncompleteReason::MaxOutputTokens
            })
        );
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
        let (mut stream, _) = EventStream::new(false, StdRng::seed_from_u64(0));
        let delta = PieceKind::Delta("more".to_string());
        assert!(matches!(
            stream.consume_piece(piece(delta.clone())),
            Err(EventStreamError::Misplaced(_))
        ));
        let end = PieceKind::End { cut_off: false };
        stream.consume_piece(piece(end)).unwrap();
        stream.complete().unwrap();
        assert!(matches!(
            stream.consume_piece(piece(delta)),
            Err(EventStreamError::Ended)
        ));
    }

    /// What streams of a generation is its text up to its first call, and
    /// none of the calls or what follows them.
    #[test]
    fn a_generation_streams_its_text_up_to_its_first_call() {
        let (mut stream, _) = EventStream::new(false, StdRng::seed_from_u64(0));
        let response = format!("<think>\nHmm.\n</think>\n\nLet me check.\n{CALL}");
        let events = generation(&mut stream, pieces(PROMPT, &response));
        let mut text = GenerationText::default();
        let streamed: String = events.iter().flat_map(|event| text.push(event)).collect();
        // The formatting before the call streams if it shares a token with the
        // text before it.
        assert_eq!(
            streamed.trim_end(),
            "<think>\nHmm.\n</think>\n\nLet me check."
        );
        let events = stream.complete().unwrap();
        assert!(events.iter().all(|event| text.push(event).is_empty()));
    }

    const CALL: &str = "<tool_call>\n{\"name\": \"get_weather\", \"arguments\": {\"city\": \"Oslo\"}}\n</tool_call><|im_end|>";
    const ANSWER: &str = "It's cloudy.<|im_end|>";

    /// A response in which we run the call the model makes, and the model
    /// answers with what the call returned.
    fn run_call(result: Result<String, McpCallError>) -> Vec<StreamEvent> {
        let (mut stream, mut events) = EventStream::new(true, StdRng::seed_from_u64(0));
        events.extend(generation(&mut stream, pieces(PROMPT, CALL)));
        events.extend(stream.call_result(OutputIndex(0), result).unwrap());
        events.extend(generation(&mut stream, pieces(PROMPT, ANSWER)));
        events.extend(stream.complete().unwrap());
        events
    }

    /// A call we run streams as the Responses API streams an MCP call.
    #[test]
    fn calls_we_run_stream_as_mcp_calls() {
        let events = run_call(Ok("17°C and cloudy".to_string()));
        assert_eq!(
            output(&consume(&events)),
            [
                ("mcp_call", r#"get_weather{"city":"Oslo"}"#.to_string()),
                ("message", "It's cloudy.".to_string()),
            ]
        );

        // With repeated deltas as one.
        assert_eq!(
            types(&events),
            [
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.mcp_call.in_progress",
                "response.mcp_call_arguments.delta",
                "response.mcp_call_arguments.done",
                "response.mcp_call.completed",
                "response.output_item.done",
                "response.output_item.added",
                "response.content_part.added",
                "response.output_text.delta",
                "response.output_text.done",
                "response.content_part.done",
                "response.output_item.done",
                "response.completed",
            ]
        );
    }

    #[test]
    fn a_call_that_fails_fails_its_item() {
        let error = McpCallError::ToolExecution {
            content: json!("Boom"),
        };
        let events = run_call(Err(error.clone()));
        assert!(types(&events).contains(&"response.mcp_call.failed".to_string()));
        let call = &consume(&events).output[0];
        assert_eq!(call.status, Status::Failed);
        assert!(matches!(
            &call.kind,
            ItemKind::McpCall(McpCallItem { result: Some(Err(e)), .. }) if *e == error
        ));
    }

    /// A call's result is written as the Responses API has it: what the call
    /// returned as `output`, or what went wrong as `error`, with the other
    /// `null`.
    #[test]
    fn call_results_are_written_as_the_api_has_them() {
        let written = |result| {
            let events = run_call(result);
            serde_json::to_value(&consume(&events).output[0]).unwrap()
        };
        let call = written(Ok("17°C and cloudy".to_string()));
        assert_eq!(call["output"], json!("17°C and cloudy"));
        assert_eq!(call["error"], Value::Null);

        let content = json!([{ "type": "text", "text": "Repository not found." }]);
        let call = written(Err(McpCallError::ToolExecution {
            content: content.clone(),
        }));
        assert_eq!(call["output"], Value::Null);
        assert_eq!(
            call["error"],
            json!({ "type": "mcp_tool_execution_error", "content": content })
        );
    }

    /// A call's end tag finishes its arguments, and the end of the generation
    /// that made it goes on the next event, which is the call's result.
    #[test]
    fn tokens_between_generations_go_on_the_next_event() {
        let events = run_call(Ok(String::new()));
        let arguments_done = tokens_of(&events, "response.mcp_call_arguments.done");
        assert!(
            arguments_done[0].contains("</tool_call>"),
            "{arguments_done:?}"
        );
        assert_eq!(
            tokens_of(&events, "response.mcp_call.completed"),
            ["<|im_end|>"]
        );
        assert_eq!(tokens_of(&events, "response.completed"), ["<|im_end|>"]);
    }

    #[test]
    fn usage_counts_every_generation() {
        let model = qwen3_vocab();
        let generated: u64 = [CALL, ANSWER]
            .iter()
            .map(|response| {
                model
                    .vocab()
                    .tokenize(response.as_bytes(), false, true)
                    .len() as u64
            })
            .sum();
        let usage = consume(&run_call(Ok(String::new()))).usage.unwrap();
        assert_eq!(usage.input_tokens, 2 * INPUT_TOKENS);
        assert_eq!(usage.output_tokens, generated);
    }

    #[test]
    fn only_running_calls_get_results_and_only_finished_responses_end() {
        let (mut stream, _) = EventStream::new(true, StdRng::seed_from_u64(0));
        generation(&mut stream, pieces(PROMPT, CALL));
        assert!(matches!(
            stream.complete(),
            Err(EventStreamError::Unfinished)
        ));
        assert!(matches!(
            stream.call_result(OutputIndex(1), Ok(String::new())),
            Err(EventStreamError::NotRunning(_))
        ));
        stream
            .call_result(OutputIndex(0), Ok(String::new()))
            .unwrap();
        assert!(matches!(
            stream.call_result(OutputIndex(0), Ok(String::new())),
            Err(EventStreamError::NotRunning(_))
        ));
        stream.complete().unwrap();
    }
}
