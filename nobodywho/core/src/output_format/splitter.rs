use super::{ParseError, ResolvedFormat};
use crate::tool_calling::{Tool, ToolCall};
use encoding_rs::{Decoder, UTF_8};
use llama_cpp_2::token::LlamaToken;

/// A stretch of a response. Text and reasoning come a token at a time, tool
/// calls a block at a time.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    Text(String),
    Thinking(String),
    Calls(Vec<ToolCall>),
    /// A block of tool calls that couldn't be read. Contains all the text it was
    /// written with.
    Malformed {
        text: String,
        error: ParseError,
    },
    /// A marker that doesn't make sense in context. E.g. an end marker with nothing to end, as its text.
    Stray(&'static str),
    /// The model ended its response. A response cut off before this, e.g. by a
    /// token limit, never gets one.
    End,
}

/// Splits a response into text, reasoning, and tool calls as it's generated.
pub struct Splitter<'a> {
    format: &'a ResolvedFormat,
    tools: &'a [Tool],
    state: State,
    /// Holds a character split across tokens until the rest of it arrives.
    decoder: Decoder,
}

#[derive(Debug)]
enum State {
    Text,
    Thinking {
        /// The reasoning read so far while it could still be the label, and
        /// `None` once it's clear whether it is.
        buffer: Option<String>,
    },
    /// Collecting the text of a block of tool calls.
    ToolCalls {
        buffer: String,
    },
    Ended,
}

impl<'a> Splitter<'a> {
    pub(super) fn new(format: &'a ResolvedFormat, tools: &'a [Tool], thinking: bool) -> Self {
        let mut splitter = Splitter {
            format,
            tools,
            state: State::Text,
            decoder: UTF_8.new_decoder_without_bom_handling(),
        };
        if thinking {
            splitter.state = splitter.thinking();
        }
        splitter
    }

    /// Takes the next token and the bytes it decodes to, and returns the
    /// stretches it finishes. There's nothing after the end of generation, so
    /// a token pushed then panics in debug builds and is ignored otherwise.
    pub fn push(&mut self, token: LlamaToken, bytes: &[u8]) -> Vec<Piece> {
        let is_end_of_generation = self.format.end_of_generation.contains(&token);
        let is_tool_call_begin = token == self.format.tool_calls.begin;
        let is_tool_call_end = Some(token) == self.format.tool_calls.end;
        let is_thinking_begin = self.format.thinking.is_some_and(|t| token == t.begin);
        let is_thinking_end = self.format.thinking.is_some_and(|t| token == t.end);

        // Which tokens are markers depends on where the response is, e.g. a
        // tool call's begin inside reasoning is just reasoning.
        let is_marker = match self.state {
            State::Ended => {
                if cfg!(debug_assertions) {
                    panic!("a token was pushed after the end of generation");
                }
                return vec![];
            }
            _ if is_end_of_generation => true,
            State::Text => {
                is_tool_call_begin || is_tool_call_end || is_thinking_begin || is_thinking_end
            }
            State::Thinking { .. } => is_thinking_end,
            State::ToolCalls { .. } => is_tool_call_begin || is_tool_call_end,
        };
        if !is_marker {
            let text = self.decode(bytes, false);
            return self.add_text(&text);
        }

        // A marker ends the stretch before it, along with any character that
        // stretch left unfinished.
        let text = self.decode(&[], true);
        let mut pieces = self.add_text(&text);
        if is_end_of_generation {
            pieces.extend(self.close_stretch());
            pieces.push(Piece::End);
            self.state = State::Ended;
            return pieces;
        }
        match &self.state {
            State::Text if is_tool_call_begin => {
                self.state = State::ToolCalls {
                    buffer: String::new(),
                };
            }
            State::Text if is_thinking_begin => self.state = self.thinking(),
            State::Text => {
                let marker = if is_thinking_end {
                    self.format.format.thinking().map(|t| t.end)
                } else {
                    self.format.format.tool_calls().end
                };
                pieces.push(Piece::Stray(
                    marker.expect("only markers the format has match"),
                ));
            }
            State::Thinking { .. } => {
                pieces.extend(self.flush_label());
                self.state = State::Text;
            }
            // Both end the block, but only `end` is part of its text. A block
            // with no end runs until the next one begins.
            State::ToolCalls { buffer } => {
                pieces.push(self.read_tool_calls(buffer, is_tool_call_end));
                self.state = if is_tool_call_end {
                    State::Text
                } else {
                    State::ToolCalls {
                        buffer: String::new(),
                    }
                };
            }
            State::Ended => unreachable!("returned above"),
        }
        pieces
    }

    /// Whether the response is in a block of tool calls, which is where a
    /// grammar would hold the model to the format.
    pub fn in_tool_calls(&self) -> bool {
        matches!(self.state, State::ToolCalls { .. })
    }

    /// Ends a response cut off before the model ended it, reading any block
    /// of tool calls it left open.
    pub fn finish(&mut self) -> Vec<Piece> {
        let text = self.decode(&[], true);
        let mut pieces = self.add_text(&text);
        pieces.extend(self.close_stretch());
        self.state = State::Ended;
        pieces
    }

    /// Decodes a token's bytes. `last` gives up on a character left
    /// unfinished, and readies the decoder for a new stretch.
    fn decode(&mut self, bytes: &[u8], last: bool) -> String {
        let capacity = self
            .decoder
            .max_utf8_buffer_length(bytes.len())
            .expect("a token's bytes fit in memory");
        let mut text = String::with_capacity(capacity);
        let _ = self.decoder.decode_to_string(bytes, &mut text, last);
        if last {
            self.decoder = UTF_8.new_decoder_without_bom_handling();
        }
        text
    }

    /// Adds text to the stretch the response is in.
    fn add_text(&mut self, text: &str) -> Vec<Piece> {
        let label = self.label();
        match &mut self.state {
            State::Text => text_piece(Piece::Text, text),
            State::Thinking { buffer: None } => text_piece(Piece::Thinking, text),
            State::Thinking {
                buffer: Some(buffer),
            } => {
                buffer.push_str(text);
                if label.starts_with(buffer.as_str()) && buffer.len() < label.len() {
                    return vec![];
                }
                let reasoning = buffer.strip_prefix(label).unwrap_or(buffer).to_string();
                self.state = State::Thinking { buffer: None };
                text_piece(Piece::Thinking, &reasoning)
            }
            State::ToolCalls { buffer } => {
                buffer.push_str(text);
                vec![]
            }
            State::Ended => vec![],
        }
    }

    /// Whatever the stretch the response is in still holds, when it ends
    /// without its end marker.
    fn close_stretch(&mut self) -> Vec<Piece> {
        match &self.state {
            State::ToolCalls { buffer } => vec![self.read_tool_calls(buffer, false)],
            State::Thinking { .. } => self.flush_label(),
            State::Text | State::Ended => vec![],
        }
    }

    fn thinking(&self) -> State {
        let buffer = (!self.label().is_empty()).then(String::new);
        State::Thinking { buffer }
    }

    fn label(&self) -> &'static str {
        self.format.format.thinking().map_or("", |t| t.label)
    }

    /// Reasoning held back while it could have been the label.
    fn flush_label(&mut self) -> Vec<Piece> {
        match &mut self.state {
            State::Thinking { buffer } => {
                let reasoning = buffer.take().unwrap_or_default();
                text_piece(Piece::Thinking, &reasoning)
            }
            _ => vec![],
        }
    }

    /// `closed` is whether the block got its end marker, which only matters
    /// for giving back the text of one that couldn't be read.
    fn read_tool_calls(&self, text: &str, closed: bool) -> Piece {
        match self.format.parse_tool_calls(text, self.tools) {
            Ok(calls) => Piece::Calls(calls),
            Err(error) => {
                let syntax = self.format.format.tool_calls();
                let end = syntax.end.filter(|_| closed).unwrap_or("");
                let text = format!("{}{text}{end}", syntax.begin);
                Piece::Malformed { text, error }
            }
        }
    }
}

fn text_piece(piece: fn(String) -> Piece, text: &str) -> Vec<Piece> {
    if text.is_empty() {
        vec![]
    } else {
        vec![piece(text.to_string())]
    }
}
