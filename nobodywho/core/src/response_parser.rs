//! Reads a model's generations as the events of a Responses API stream. This
//! is the only part of reading a response that depends on the model.

use crate::event_stream::{event::StreamEvent, EventStream};
use crate::output_format::{Piece, PieceKind, ResolvedFormat, Splitter, Warning};
use crate::tool_calling::Tool;
use llama_cpp_2::{model::LlamaModel, token::LlamaToken, TokenToStringError};
use rand::rngs::ThreadRng;
use tracing::{debug, warn};

/// Reads one generation into a response's event stream.
pub(crate) struct ResponseParser<'a> {
    model: &'a LlamaModel,
    splitter: Splitter<'a>,
    rng: ThreadRng,
    ended: bool,
    /// Whether the model wrote a block of tool calls that can't be read.
    unreadable: bool,
    /// Whether a grammar holds the model to the format's tool calls.
    has_grammar: bool,
}

impl<'a> ResponseParser<'a> {
    /// Starts reading a generation that continues `prompt`.
    pub fn new(
        model: &'a LlamaModel,
        format: Option<&'a ResolvedFormat>,
        tools: &'a [Tool],
        prompt: &str,
    ) -> Self {
        let splitter = match format {
            Some(format) => format.splitter(tools, prompt),
            None => {
                debug!("No output format, so the response is read as plain text");
                Splitter::plain(model)
            }
        };
        ResponseParser {
            model,
            splitter,
            rng: rand::rng(),
            ended: false,
            unreadable: false,
            has_grammar: format.is_some() && !tools.is_empty(),
        }
    }

    /// Takes the next token and returns the events it finishes.
    pub fn push(
        &mut self,
        token: LlamaToken,
        stream: &mut EventStream,
    ) -> Result<Vec<StreamEvent>, TokenToStringError> {
        let bytes = self.bytes(token)?;
        let pieces = self.splitter.push(token, &bytes);
        Ok(self.events_of(pieces, stream))
    }

    /// Whether the generation is in a block of tool calls, which is where a
    /// grammar would hold the model to the format.
    pub fn in_tool_calls(&self) -> bool {
        self.splitter.in_tool_calls()
    }

    /// Whether the model has ended the generation.
    pub fn ended(&self) -> bool {
        self.ended
    }

    /// Ends the generation, as cut off if the model hasn't ended it, and
    /// returns its last events along with what the history keeps of it, as
    /// [`Splitter::written`] says.
    pub fn finish(mut self, stream: &mut EventStream) -> (Vec<StreamEvent>, String) {
        let pieces = self.splitter.finish();
        let events = self.events_of(pieces, stream);
        // The grammar should keep a model from writing a block it can't read,
        // unless it was cut off.
        debug_assert!(
            !(self.has_grammar && self.unreadable && self.ended),
            "the model wrote a block of tool calls that can't be read"
        );
        (events, self.splitter.written())
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

    /// Hands `pieces` to `stream` and returns the events they make, logging
    /// the warnings among them and noting when the model ends the generation.
    fn events_of(&mut self, pieces: Vec<Piece>, stream: &mut EventStream) -> Vec<StreamEvent> {
        let mut events = Vec::new();
        for piece in pieces {
            match &piece.kind {
                PieceKind::Warning(Warning::Malformed(error)) => {
                    warn!(%error, "Couldn't read a block of tool calls");
                    self.unreadable = true;
                }
                PieceKind::Warning(Warning::Stray(marker)) => {
                    warn!(marker, "The model wrote an end marker with nothing to end")
                }
                PieceKind::End { cut_off } => self.ended = !cut_off,
                _ => {}
            }
            let new = stream
                .consume_piece(piece, &mut self.rng)
                .expect("the splitter's pieces fit together");
            events.extend(new);
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_stream::{
        event::EventKind,
        response::{FunctionCallItem, ItemKind, Status},
    };
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

    /// Reads `response` as one generation after `prompt`, token by token as
    /// the model would write it, and returns its events, the stream, what the
    /// history keeps of it and whether the model ended it.
    fn parse(
        model: &LlamaModel,
        format: Option<&ResolvedFormat>,
        prompt: &str,
        response: &str,
    ) -> (Vec<StreamEvent>, EventStream, String, bool) {
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
        let (mut stream, mut events) = EventStream::new(false, &mut rand::rng());
        let mut parser = ResponseParser::new(model, format, &tools, prompt);
        for token in model.str_to_token(response, AddBos::Never).unwrap() {
            events.extend(parser.push(token, &mut stream).unwrap());
        }
        let ended = parser.ended();
        let (last, written) = parser.finish(&mut stream);
        events.extend(last);
        (events, stream, written, ended)
    }

    /// What the history keeps of a generation is exactly what the model
    /// wrote, whitespace and stray markers included, and of a prompt that
    /// opened the reasoning, the reasoning it opened.
    #[test]
    fn the_history_keeps_what_the_model_wrote() {
        let model = qwen3_vocab();
        let format = ResolvedFormat::new(&Qwen3, &model).unwrap();
        let opened = format!("{PROMPT}<think>\n");
        let call = "<tool_call>\n{\"name\": \"get_weather\", \"arguments\": {\"city\": \"Oslo\"}}\n</tool_call>";
        for (prompt, response, kept) in [
            (
                PROMPT,
                "<think>\nHmm.\n</think>\n\nHi!",
                "<think>\nHmm.\n</think>\n\nHi!",
            ),
            (PROMPT, "<think>Hmm.</think>Hi!", "<think>Hmm.</think>Hi!"),
            (
                PROMPT,
                "<think>\nHmm.\n</think>\nHi!",
                "<think>\nHmm.\n</think>\nHi!",
            ),
            (
                PROMPT,
                "<think>\n\nHmm.\n\n</think>\n\n\nHi!",
                "<think>\n\nHmm.\n\n</think>\n\n\nHi!",
            ),
            (PROMPT, "  Indented, with 🦀.\n", "  Indented, with 🦀.\n"),
            (PROMPT, "Hi </think> there", "Hi </think> there"),
            (PROMPT, "Hi </think>", "Hi </think>"),
            (
                &opened,
                "Easy.\n</think>\n\nIt's 4.",
                "<think>\nEasy.\n</think>\n\nIt's 4.",
            ),
            (
                PROMPT,
                &format!("Let me check.\n{call}\nMore."),
                "Let me check.",
            ),
            (
                PROMPT,
                "Cut off mid<think>\nthought",
                "Cut off mid<think>\nthought",
            ),
        ] {
            for end in ["<|im_end|>", ""] {
                let (_, _, written, _) =
                    parse(&model, Some(&format), prompt, &format!("{response}{end}"));
                assert_eq!(written, kept, "{response:?}");
            }
        }
    }

    /// A block that's cut off is kept as the text it is, even one that reads
    /// as a call, so a stop or the token limit keeps its call from running.
    #[test]
    fn a_cut_off_block_is_text() {
        let model = qwen3_vocab();
        let format = ResolvedFormat::new(&Qwen3, &model).unwrap();
        let call =
            "<tool_call>\n{\"name\": \"get_weather\", \"arguments\": {\"city\": \"Oslo\"}}\n";
        for block in ["<tool_call>\nnot js", call] {
            let (_, stream, written, ended) = parse(&model, Some(&format), PROMPT, block);
            assert!(!ended);
            assert_eq!(written, block);
            assert!(stream
                .response()
                .output
                .iter()
                .all(|item| matches!(item.kind, ItemKind::Message(_))));
        }
    }

    /// The grammar keeps the model from ending with a block it can't read, so
    /// a debug build takes it as a bug. Otherwise it's text.
    #[test]
    #[cfg_attr(debug_assertions, should_panic(expected = "can't be read"))]
    fn an_unreadable_block_is_a_bug() {
        let model = qwen3_vocab();
        let format = ResolvedFormat::new(&Qwen3, &model).unwrap();
        let block = "<tool_call>\nnot json\n</tool_call>";
        let (_, _, written, _) =
            parse(&model, Some(&format), PROMPT, &format!("{block}<|im_end|>"));
        assert_eq!(written, block);
    }

    /// Without tools there's no grammar to hold the model to the format, so
    /// an unreadable block is only text.
    #[test]
    fn without_tools_an_unreadable_block_is_text() {
        let model = qwen3_vocab();
        let format = ResolvedFormat::new(&Qwen3, &model).unwrap();
        let block = "<tool_call>\nnot json\n</tool_call>";
        let (mut stream, _) = EventStream::new(false, &mut rand::rng());
        let mut parser = ResponseParser::new(&model, Some(&format), &[], PROMPT);
        let response = format!("{block}<|im_end|>");
        for token in model.str_to_token(&response, AddBos::Never).unwrap() {
            parser.push(token, &mut stream).unwrap();
        }
        let (_, written) = parser.finish(&mut stream);
        assert_eq!(written, block);
    }

    #[test]
    fn without_an_output_format_everything_is_text() {
        let model = qwen3_vocab();
        let (_, _, written, ended) =
            parse(&model, None, PROMPT, "<think>\nHi 🦀</think><|im_end|>");
        assert_eq!(written, "<think>\nHi 🦀</think>");
        assert!(ended);

        let (_, _, written, ended) = parse(&model, None, PROMPT, "Cut off");
        assert_eq!(written, "Cut off");
        assert!(!ended);
    }

    #[test]
    fn calls_are_items_of_their_own() {
        let model = qwen3_vocab();
        let format = ResolvedFormat::new(&Qwen3, &model).unwrap();
        let call = "<tool_call>\n{\"name\": \"get_weather\", \"arguments\": {\"city\": \"Oslo\"}}\n</tool_call>";
        let (_, stream, _, ended) = parse(
            &model,
            Some(&format),
            PROMPT,
            &format!("Let me check.\n{call}<|im_end|>"),
        );
        assert!(ended);
        assert!(matches!(
            &stream.response().output[1].kind,
            ItemKind::FunctionCall(FunctionCallItem { name, arguments, .. })
                if name == "get_weather"
                    && serde_json::from_str::<serde_json::Value>(arguments).unwrap()
                        == json!({ "city": "Oslo" })
        ));
    }

    /// A generation doesn't end the response, which can go on with another.
    #[test]
    fn a_generation_knows_whether_the_model_ended_it() {
        let model = qwen3_vocab();
        let format = ResolvedFormat::new(&Qwen3, &model).unwrap();
        for (response, model_ended) in [("Done.<|im_end|>", true), ("Cut", false)] {
            let (events, stream, _, ended) = parse(&model, Some(&format), PROMPT, response);
            assert_eq!(ended, model_ended);
            assert!(!events.iter().any(|event| matches!(
                event.kind,
                EventKind::Completed { .. } | EventKind::Incomplete { .. }
            )));
            assert!(stream
                .response()
                .output
                .iter()
                .all(|item| item.status == Status::Completed));
        }
    }
}
