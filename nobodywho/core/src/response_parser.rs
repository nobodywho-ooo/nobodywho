//! Reads a response from the tokens a model generates, as the events of a
//! Responses API stream. This is the only part of reading a response that
//! depends on the model.

use crate::event_stream::{
    event::StreamEvent,
    response::{FunctionCallItem, ItemKind, MessageItem, ReasoningItem},
    EventStream,
};
use crate::output_format::{Piece, PieceKind, ResolvedFormat, Splitter, Warning};
use crate::tool_calling::{Tool, ToolCall};
use llama_cpp_2::{model::LlamaModel, token::LlamaToken, TokenToStringError};
use rand::rngs::ThreadRng;
use tracing::{debug, error, warn};

pub(crate) struct ResponseParser<'a> {
    model: &'a LlamaModel,
    format: Option<&'a ResolvedFormat>,
    splitter: Splitter<'a>,
    events: EventStream,
    rng: ThreadRng,
}

/// A finished response.
pub(crate) struct ParsedResponse {
    /// The events the response ended with.
    pub events: Vec<StreamEvent>,
    /// What the response said before its tool calls, with any reasoning as the
    /// model writes it.
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
}

impl<'a> ResponseParser<'a> {
    /// Starts reading the response to `prompt`, of `prompt_tokens` tokens,
    /// along with the events that announce it.
    pub fn new(
        model: &'a LlamaModel,
        format: Option<&'a ResolvedFormat>,
        tools: &'a [Tool],
        prompt: &str,
        prompt_tokens: usize,
    ) -> (Self, Vec<StreamEvent>) {
        let splitter = match format {
            Some(format) => format.splitter(tools, prompt),
            None => {
                debug!("No output format, so the response is read as plain text");
                Splitter::plain(model)
            }
        };
        let mut rng = rand::rng();
        let (events, announced) = EventStream::new(prompt_tokens as u64, &mut rng);
        let parser = ResponseParser {
            model,
            format,
            splitter,
            events,
            rng,
        };
        (parser, announced)
    }

    /// Takes the next token and returns the events it finishes.
    pub fn push(&mut self, token: LlamaToken) -> Result<Vec<StreamEvent>, TokenToStringError> {
        let bytes = self.bytes(token)?;
        let pieces = self.splitter.push(token, &bytes);
        Ok(self.events_of(pieces))
    }

    /// Whether the response is in a block of tool calls, which is where a
    /// grammar would hold the model to the format.
    pub fn in_tool_calls(&self) -> bool {
        self.splitter.in_tool_calls()
    }

    /// Ends the response, as cut off if the model hasn't ended it.
    pub fn finish(mut self) -> ParsedResponse {
        let pieces = self.splitter.finish();
        let events = self.events_of(pieces);

        let mut content = String::new();
        let mut tool_calls = Vec::new();
        for item in self.events.response().output() {
            match &item.kind {
                ItemKind::FunctionCall(FunctionCallItem {
                    name, arguments, ..
                }) => tool_calls.push(ToolCall {
                    name: name.clone(),
                    arguments: serde_json::from_str(arguments)
                        .expect("the splitter writes arguments as JSON"),
                }),
                // Whatever follows the calls is left out, like the calls.
                _ if !tool_calls.is_empty() => {}
                ItemKind::Reasoning(ReasoningItem { content: parts, .. }) => {
                    let reasoning: String = parts.iter().map(|part| part.text.as_str()).collect();
                    let format = self.format.expect("only an output format finds reasoning");
                    content.push_str(&format.write_reasoning(&reasoning));
                }
                ItemKind::Message(MessageItem { content: parts, .. }) => {
                    content.extend(parts.iter().map(|part| part.text.as_str()));
                }
                ItemKind::FunctionCallOutput(_) => {}
            }
        }
        ParsedResponse {
            events,
            content,
            tool_calls,
        }
    }

    fn bytes(&self, token: LlamaToken) -> Result<Vec<u8>, TokenToStringError> {
        match self.model.token_to_piece_bytes(token, 64, true, None) {
            Err(TokenToStringError::InsufficientBufferSpace(needed)) => {
                let size = (-needed).try_into().expect("the needed size is positive");
                self.model.token_to_piece_bytes(token, size, true, None)
            }
            bytes => bytes,
        }
    }

    fn events_of(&mut self, pieces: Vec<Piece>) -> Vec<StreamEvent> {
        let mut events = Vec::new();
        for piece in pieces {
            match &piece.kind {
                PieceKind::Warning(Warning::Malformed(error)) => {
                    warn!(%error, "Couldn't read a block of tool calls")
                }
                PieceKind::Warning(Warning::Stray(marker)) => {
                    warn!(marker, "The model wrote an end marker with nothing to end")
                }
                _ => {}
            }
            match self.events.consume_piece(piece, &mut self.rng) {
                Ok(new) => events.extend(new),
                Err(error) => error!(%error, "The splitter's pieces don't fit together"),
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_stream::event::EventKind;
    use crate::output_format::Qwen3;
    use llama_cpp_2::model::{params::LlamaModelParams, AddBos};
    use serde_json::json;
    use std::sync::Arc;

    fn qwen3_vocab() -> LlamaModel {
        let path = std::env::var("TEST_MODEL").unwrap_or_else(|_| "model.gguf".to_string());
        let params = LlamaModelParams::default().with_vocab_only(true);
        LlamaModel::load_from_file(&crate::llm::LLAMA_BACKEND, &path, &params)
            .unwrap_or_else(|e| panic!("failed to load vocabulary from {path}: {e}"))
    }

    const PROMPT: &str = "<|im_start|>user\nHi<|im_end|>\n<|im_start|>assistant\n";

    /// Parses `response`, token by token as the model would write it.
    fn parse(
        model: &LlamaModel,
        format: Option<&ResolvedFormat>,
        response: &str,
    ) -> (Vec<StreamEvent>, ParsedResponse) {
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
        let (mut parser, mut events) = ResponseParser::new(model, format, &tools, PROMPT, 7);
        for token in model.str_to_token(response, AddBos::Never).unwrap() {
            events.extend(parser.push(token).unwrap());
        }
        let parsed = parser.finish();
        events.extend(parsed.events.iter().cloned());
        (events, parsed)
    }

    fn ends_with(events: &[StreamEvent]) -> &EventKind {
        &events.last().unwrap().kind
    }

    #[test]
    fn without_an_output_format_everything_is_text() {
        let model = qwen3_vocab();
        let (events, parsed) = parse(&model, None, "<think>\nHi 🦀</think><|im_end|>");
        assert_eq!(parsed.content, "<think>\nHi 🦀</think>");
        assert!(parsed.tool_calls.is_empty());
        assert!(matches!(ends_with(&events), EventKind::Completed { .. }));

        let (events, parsed) = parse(&model, None, "Cut off");
        assert_eq!(parsed.content, "Cut off");
        assert!(matches!(ends_with(&events), EventKind::Incomplete { .. }));
    }

    #[test]
    fn content_is_what_came_before_the_calls_as_the_model_writes_it() {
        let model = qwen3_vocab();
        let format = ResolvedFormat::new(&Qwen3, &model).unwrap();
        let call = "<tool_call>\n{\"name\": \"get_weather\", \"arguments\": {\"city\": \"Oslo\"}}\n</tool_call>";
        let response = format!("<think>\nHmm.\n</think>\n\nLet me check.\n{call}<|im_end|>");
        let (_, parsed) = parse(&model, Some(&format), &response);
        assert_eq!(parsed.content, "<think>\nHmm.\n</think>\n\nLet me check.");
        assert_eq!(
            parsed.tool_calls,
            [ToolCall {
                name: "get_weather".into(),
                arguments: json!({ "city": "Oslo" }),
            }]
        );
    }

    #[test]
    fn a_response_ends_with_the_event_that_ends_it() {
        let model = qwen3_vocab();
        let format = ResolvedFormat::new(&Qwen3, &model).unwrap();
        let (events, _) = parse(&model, Some(&format), "Done.<|im_end|>");
        assert!(matches!(ends_with(&events), EventKind::Completed { .. }));
        let (events, _) = parse(&model, Some(&format), "Cut");
        assert!(matches!(ends_with(&events), EventKind::Incomplete { .. }));
    }
}
