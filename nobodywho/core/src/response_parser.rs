//! Reads a model's generations: the text to stream, the tool calls, and what
//! the history keeps. This is the only part of reading a response that depends
//! on the model.

use crate::output_format::{Item, ModelOutput, Piece, PieceKind, Splitter, Warning};
use crate::tool_calling::{Tool, ToolCall};
use llama_cpp_2::token::LlamaToken;
use tracing::{debug, warn};

/// Reads one generation.
pub(crate) struct ResponseParser {
    splitter: Splitter,
    ended: bool,
    /// Whether the model has begun its calls, from which on nothing streams.
    calls_begun: bool,
    calls: Vec<ToolCall>,
    /// The call being read, with its arguments so far.
    call: Option<(String, String)>,
    /// Whether the model wrote a block of tool calls that can't be read.
    unreadable: bool,
    /// Whether a grammar holds the model to the format's tool calls.
    has_grammar: bool,
}

/// A finished generation.
pub(crate) struct Generation {
    /// What the history keeps of it, as [`Splitter::written`] says.
    pub written: String,
    pub calls: Vec<ToolCall>,
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
            calls_begun: false,
            calls: Vec::new(),
            call: None,
            unreadable: false,
            has_grammar,
        }
    }

    /// Takes the next token and the bytes it decodes to, and returns the text
    /// it finishes that streams.
    pub fn push(&mut self, token: LlamaToken, bytes: &[u8]) -> Vec<String> {
        let pieces = self.splitter.push(token, bytes);
        self.read(pieces)
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
    /// returns the last of its text that streams, along with the generation.
    pub fn finish(mut self) -> (Vec<String>, Generation) {
        let pieces = self.splitter.finish();
        let text = self.read(pieces);
        // The grammar should keep a model from writing a block it can't read,
        // unless it was cut off.
        debug_assert!(
            !(self.has_grammar && self.unreadable && self.ended),
            "the model wrote a block of tool calls that can't be read"
        );
        let generation = Generation {
            written: self.splitter.written(),
            calls: self.calls,
        };
        (text, generation)
    }

    /// Reads `pieces` for their calls, logging the warnings among them and
    /// noting when the model ends the generation, and returns the texts of
    /// their tokens that stream: everything the model writes up to its first
    /// call, but for the end of generation.
    fn read(&mut self, pieces: Vec<Piece>) -> Vec<String> {
        let mut text = Vec::new();
        for Piece { kind, tokens } in pieces {
            match kind {
                PieceKind::Warning(Warning::Malformed(error)) => {
                    warn!(%error, "Couldn't read a block of tool calls");
                    self.unreadable = true;
                }
                PieceKind::Warning(Warning::Stray(marker)) => {
                    warn!(marker, "The model wrote an end marker with nothing to end")
                }
                PieceKind::End { cut_off } => {
                    self.ended = !cut_off;
                    continue;
                }
                PieceKind::Open(Item::ToolCall { name }) => {
                    self.calls_begun = true;
                    self.call = Some((name, String::new()));
                }
                PieceKind::Delta(delta) => {
                    if let Some((_, arguments)) = &mut self.call {
                        arguments.push_str(&delta);
                    }
                }
                PieceKind::Close => {
                    if let Some((name, arguments)) = self.call.take() {
                        self.calls.push(ToolCall {
                            name,
                            arguments: serde_json::from_str(&arguments)
                                .expect("the splitter writes arguments as JSON"),
                        });
                    }
                }
                PieceKind::Open(_) => {}
            }
            if !self.calls_begun {
                text.extend(tokens.into_iter().map(|token| token.text));
            }
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output_format::{qwen3, ResolvedFormat};
    use crate::test_utils::load_test_vocab;
    use llama_cpp_2::model::LlamaModel;
    use serde_json::json;
    use std::sync::Arc;

    /// Pushes `token` with the bytes it decodes to, as generation does.
    fn push(parser: &mut ResponseParser, model: &LlamaModel, token: LlamaToken) -> Vec<String> {
        parser.push(token, &model.vocab().token_to_piece(token, true, None))
    }

    const PROMPT: &str = "<|im_start|>user\nHi<|im_end|>\n<|im_start|>assistant\n";

    /// Reads `response` as one generation after `prompt`, token by token as
    /// the model would write it, and returns the text that streams, the
    /// generation and whether the model ended it.
    fn parse(
        model: &LlamaModel,
        output: ModelOutput,
        prompt: &str,
        response: &str,
    ) -> (String, Generation, bool) {
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
        let mut parser = ResponseParser::new(output, tools.to_vec(), prompt);
        let mut streamed = String::new();
        for token in model.vocab().tokenize(response.as_bytes(), false, true) {
            streamed.extend(push(&mut parser, model, token));
        }
        let ended = parser.ended();
        let (last, generation) = parser.finish();
        streamed.extend(last);
        (streamed, generation, ended)
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
                let (_, generation, _) =
                    parse(&model, format.clone(), prompt, &format!("{response}{end}"));
                assert_eq!(generation.written, kept, "{response:?}");
            }
        }
    }

    /// What streams is exactly the model's text up to its first call.
    #[test]
    fn the_text_up_to_the_calls_streams() {
        let model = load_test_vocab();
        let format =
            ModelOutput::Formatted(Box::new(ResolvedFormat::new(qwen3(), &model).unwrap()));
        let call = "<tool_call>\n{\"name\": \"get_weather\", \"arguments\": {\"city\": \"Oslo\"}}\n</tool_call>";
        let (streamed, _, _) = parse(
            &model,
            format.clone(),
            PROMPT,
            "<think>\nHmm.\n</think>\n\nHi!<|im_end|>",
        );
        assert_eq!(streamed, "<think>\nHmm.\n</think>\n\nHi!");
        let (streamed, _, _) = parse(
            &model,
            format,
            PROMPT,
            &format!("Let me check{call}\nMore.<|im_end|>"),
        );
        assert_eq!(streamed, "Let me check");
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
            let (_, generation, ended) = parse(&model, format.clone(), PROMPT, block);
            assert!(!ended);
            assert_eq!(generation.written, block);
            assert!(generation.calls.is_empty());
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
        let (_, generation, _) = parse(&model, format, PROMPT, &format!("{block}<|im_end|>"));
        assert_eq!(generation.written, block);
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
        let (_, generation) = parser.finish();
        assert_eq!(generation.written, block);
    }

    #[test]
    fn without_an_output_format_everything_is_text() {
        let model = load_test_vocab();
        let plain = ModelOutput::plain(&model);
        let (_, generation, ended) = parse(
            &model,
            plain.clone(),
            PROMPT,
            "<think>\nHi 🦀</think><|im_end|>",
        );
        assert_eq!(generation.written, "<think>\nHi 🦀</think>");
        assert!(ended);

        let (_, generation, ended) = parse(&model, plain, PROMPT, "Cut off");
        assert_eq!(generation.written, "Cut off");
        assert!(!ended);
    }

    #[test]
    fn calls_are_read_from_their_blocks() {
        let model = load_test_vocab();
        let format =
            ModelOutput::Formatted(Box::new(ResolvedFormat::new(qwen3(), &model).unwrap()));
        let call = "<tool_call>\n{\"name\": \"get_weather\", \"arguments\": {\"city\": \"Oslo\"}}\n</tool_call>";
        let (_, generation, ended) = parse(
            &model,
            format,
            PROMPT,
            &format!("Let me check.\n{call}<|im_end|>"),
        );
        assert!(ended);
        assert_eq!(
            generation.calls,
            [ToolCall {
                name: "get_weather".into(),
                arguments: json!({ "city": "Oslo" }),
            }]
        );
    }
}
