//! Reads a model's generations as pieces, which an
//! [`EventStream`](crate::event_stream::EventStream) turns into events. This is
//! the only part of reading a response that depends on the model.

use crate::output_format::{ModelOutput, Piece, PieceKind, Splitter, Warning};
use crate::tool_calling::Tool;
use llama_cpp_2::token::LlamaToken;
use tracing::{debug, warn};

/// Reads one generation.
pub(crate) struct ResponseParser {
    splitter: Splitter,
    ended: bool,
    /// Whether the model wrote a block of tool calls that can't be read.
    unreadable: bool,
    /// Whether a grammar holds the model to the format's tool calls.
    has_grammar: bool,
}

impl ResponseParser {
    /// Starts reading a generation that continues `prompt`.
    pub fn new(output: ModelOutput, tools: Vec<Tool>, prompt: &str) -> Self {
        if output.resolved_format().is_none() {
            debug!("No output format, so the response is read as plain text");
        }
        let is_tools_empty = tools.is_empty();
        let has_grammar = output.resolved_format().is_some() && !is_tools_empty;
        ResponseParser {
            splitter: output.splitter(tools, prompt),
            ended: false,
            unreadable: false,
            has_grammar,
        }
    }

    /// Takes the next token and the bytes it decodes to, and returns the
    /// pieces it finishes.
    pub fn push(&mut self, token: LlamaToken, bytes: &[u8]) -> Vec<Piece> {
        let (pieces, warning) = self.splitter.push(token, bytes);
        self.log_warning(warning);
        self.note_end(&pieces);
        pieces
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
    /// returns its last pieces along with what the history keeps of it, as
    /// [`Splitter::written`] says.
    pub fn finish(mut self) -> (Vec<Piece>, String) {
        let (pieces, warning) = self.splitter.finish();
        self.log_warning(warning);
        self.note_end(&pieces);
        // The grammar should keep a model from writing a block it can't read,
        // unless it was cut off.
        debug_assert!(
            !(self.has_grammar && self.unreadable && self.ended),
            "the model wrote a block of tool calls that can't be read"
        );
        (pieces, self.splitter.written())
    }

    /// Logs `warning`, noting a block of calls that can't be read.
    fn log_warning(&mut self, warning: Option<Warning>) {
        match warning {
            Some(Warning::Malformed(error)) => {
                warn!(%error, "Couldn't read a block of tool calls");
                self.unreadable = true;
            }
            Some(Warning::Stray(marker)) => {
                warn!(marker, "The model wrote an end marker with nothing to end")
            }
            None => {}
        }
    }

    /// Notes whether `pieces` end the generation, and if so, whether the
    /// model ended it.
    fn note_end(&mut self, pieces: &[Piece]) {
        for piece in pieces {
            if let PieceKind::End { cut_off } = piece.kind {
                self.ended = !cut_off;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_stream::{
        event::{EventKind, StreamEvent},
        response::{FunctionCallItem, ItemKind, Status},
        EventStream,
    };
    use crate::output_format::{qwen3, ResolvedFormat};
    use crate::test_utils::load_test_vocab;
    use llama_cpp_2::model::LlamaModel;
    use rand::{rngs::StdRng, SeedableRng};
    use serde_json::json;
    use std::sync::Arc;

    /// Pushes `token` with the bytes it decodes to, as generation does.
    fn push(parser: &mut ResponseParser, model: &LlamaModel, token: LlamaToken) -> Vec<Piece> {
        let bytes = model.vocab().token_to_piece(token, true, None);
        parser.push(token, &bytes)
    }

    /// The events `stream` makes of `pieces`.
    fn events(stream: &mut EventStream, pieces: Vec<Piece>) -> Vec<StreamEvent> {
        pieces
            .into_iter()
            .flat_map(|piece| stream.consume_piece(piece).unwrap())
            .collect()
    }

    const PROMPT: &str = "<|im_start|>user\nHi<|im_end|>\n<|im_start|>assistant\n";

    /// Reads `response` as one generation after `prompt`, token by token as
    /// the model would write it, and returns its events, the stream, what the
    /// history keeps of it and whether the model ended it.
    fn parse(
        model: &LlamaModel,
        output: ModelOutput,
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
        let (mut stream, mut all) = EventStream::new(false, StdRng::seed_from_u64(0));
        let mut parser = ResponseParser::new(output, tools.to_vec(), prompt);
        for token in model.vocab().tokenize(response.as_bytes(), false, true) {
            let pieces = push(&mut parser, model, token);
            all.extend(events(&mut stream, pieces));
        }
        let ended = parser.ended();
        let (last, written) = parser.finish();
        all.extend(events(&mut stream, last));
        (all, stream, written, ended)
    }

    /// What the history keeps of a generation is exactly what the model
    /// wrote, whitespace and stray markers included, and of a prompt that
    /// opened the reasoning, the reasoning it opened.
    #[test]
    fn the_history_keeps_what_the_model_wrote() {
        let model = load_test_vocab();
        let format =
            ModelOutput::Formatted(Box::new(ResolvedFormat::new(qwen3(), &model).unwrap()));
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
                    parse(&model, format.clone(), prompt, &format!("{response}{end}"));
                assert_eq!(written, kept, "{response:?}");
            }
        }
    }

    /// A block that's cut off is kept as the text it is, even one that reads
    /// as a call, so a stop or the token limit keeps its call from running.
    #[test]
    fn a_cut_off_block_is_text() {
        let model = load_test_vocab();
        let format =
            ModelOutput::Formatted(Box::new(ResolvedFormat::new(qwen3(), &model).unwrap()));
        let call =
            "<tool_call>\n{\"name\": \"get_weather\", \"arguments\": {\"city\": \"Oslo\"}}\n";
        for block in ["<tool_call>\nnot js", call] {
            let (_, stream, written, ended) = parse(&model, format.clone(), PROMPT, block);
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
        let model = load_test_vocab();
        let format =
            ModelOutput::Formatted(Box::new(ResolvedFormat::new(qwen3(), &model).unwrap()));
        let block = "<tool_call>\nnot json\n</tool_call>";
        let (_, _, written, _) = parse(&model, format, PROMPT, &format!("{block}<|im_end|>"));
        assert_eq!(written, block);
    }

    /// Without tools there's no grammar to hold the model to the format, so
    /// an unreadable block is only text.
    #[test]
    fn without_tools_an_unreadable_block_is_text() {
        let model = load_test_vocab();
        let format =
            ModelOutput::Formatted(Box::new(ResolvedFormat::new(qwen3(), &model).unwrap()));
        let block = "<tool_call>\nnot json\n</tool_call>";
        let mut parser = ResponseParser::new(format, Vec::new(), PROMPT);
        let response = format!("{block}<|im_end|>");
        for token in model.vocab().tokenize(response.as_bytes(), false, true) {
            push(&mut parser, &model, token);
        }
        let (_, written) = parser.finish();
        assert_eq!(written, block);
    }

    #[test]
    fn without_an_output_format_everything_is_text() {
        let model = load_test_vocab();
        let plain = ModelOutput::plain(&model);
        let (_, _, written, ended) = parse(
            &model,
            plain.clone(),
            PROMPT,
            "<think>\nHi 🦀</think><|im_end|>",
        );
        assert_eq!(written, "<think>\nHi 🦀</think>");
        assert!(ended);

        let (_, _, written, ended) = parse(&model, plain, PROMPT, "Cut off");
        assert_eq!(written, "Cut off");
        assert!(!ended);
    }

    #[test]
    fn calls_are_items_of_their_own() {
        let model = load_test_vocab();
        let format =
            ModelOutput::Formatted(Box::new(ResolvedFormat::new(qwen3(), &model).unwrap()));
        let call = "<tool_call>\n{\"name\": \"get_weather\", \"arguments\": {\"city\": \"Oslo\"}}\n</tool_call>";
        let (_, stream, _, ended) = parse(
            &model,
            format,
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
        let model = load_test_vocab();
        let format =
            ModelOutput::Formatted(Box::new(ResolvedFormat::new(qwen3(), &model).unwrap()));
        for (response, model_ended) in [("Done.<|im_end|>", true), ("Cut", false)] {
            let (events, stream, _, ended) = parse(&model, format.clone(), PROMPT, response);
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
