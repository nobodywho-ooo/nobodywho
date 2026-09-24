use super::{ParseError, ResolvedFormat};
use crate::tool_calling::{Tool, ToolCall};
use encoding_rs::{Decoder, UTF_8};
use llama_cpp_2::token::LlamaToken;

/// A stretch of a response. Text and reasoning come a token at a time, tool
/// calls a block at a time. Neither text nor reasoning includes the formatting
/// the template writes around markers.
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
    /// Text or reasoning, which the model writes a token at a time.
    Stretch {
        kind: Stretch,
        /// The formatting the template writes after the marker that began the
        /// stretch, while what's been read could still be it.
        opening: Option<Opening>,
        /// Text at the end that could be the formatting before the next
        /// marker, held back until it's clear whether it is.
        held: String,
    },
    /// Collecting the text of a block of tool calls.
    ToolCalls {
        buffer: String,
    },
    Ended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stretch {
    Text,
    Thinking,
}

#[derive(Debug)]
struct Opening {
    formatting: &'static str,
    read: String,
}

impl<'a> Splitter<'a> {
    /// `thinking` is `Some` when the prompt left the response reasoning, with
    /// the formatting after the reasoning's `begin` that's still to come.
    pub(super) fn new(
        format: &'a ResolvedFormat,
        tools: &'a [Tool],
        thinking: Option<&'static str>,
    ) -> Self {
        let state = match thinking {
            Some(after_begin) => stretch(Stretch::Thinking, after_begin),
            None => stretch(Stretch::Text, ""),
        };
        Splitter {
            format,
            tools,
            state,
            decoder: UTF_8.new_decoder_without_bom_handling(),
        }
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
            State::Stretch {
                kind: Stretch::Text,
                ..
            } => is_tool_call_begin || is_tool_call_end || is_thinking_begin || is_thinking_end,
            State::Stretch {
                kind: Stretch::Thinking,
                ..
            } => is_thinking_end,
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
            pieces.extend(self.close_stretch(false));
            pieces.push(Piece::End);
            self.state = State::Ended;
            return pieces;
        }
        let tool_calls = self.format.format.tool_calls();
        let thinking = self.format.format.thinking();
        match &self.state {
            State::Stretch {
                kind: Stretch::Text,
                ..
            } if is_tool_call_begin => {
                pieces.extend(self.close_stretch(true));
                self.state = State::ToolCalls {
                    buffer: String::new(),
                };
            }
            State::Stretch {
                kind: Stretch::Text,
                ..
            } if is_thinking_begin => {
                pieces.extend(self.close_stretch(false));
                self.state = stretch(Stretch::Thinking, thinking.map_or("", |t| t.after_begin));
            }
            State::Stretch {
                kind: Stretch::Text,
                ..
            } => {
                let marker = if is_thinking_end {
                    thinking.map(|t| t.end)
                } else {
                    tool_calls.end
                };
                pieces.push(Piece::Stray(
                    marker.expect("only markers the format has match"),
                ));
            }
            State::Stretch {
                kind: Stretch::Thinking,
                ..
            } => {
                pieces.extend(self.close_stretch(true));
                self.state = stretch(Stretch::Text, thinking.map_or("", |t| t.after_end));
            }
            // Both end the block, but only `end` is part of its text. A block
            // with no end runs until the next one begins.
            State::ToolCalls { buffer } => {
                pieces.push(self.read_tool_calls(buffer, is_tool_call_end));
                self.state = if is_tool_call_end {
                    stretch(Stretch::Text, tool_calls.after_end)
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
        pieces.extend(self.close_stretch(false));
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

    /// Adds text to the stretch the response is in, and returns what's
    /// certainly not formatting.
    fn add_text(&mut self, text: &str) -> Vec<Piece> {
        let before = self.formatting_before();
        match &mut self.state {
            State::Stretch {
                kind,
                opening,
                held,
            } => {
                let mut text = text.to_string();
                if let Some(opening_formatting) = opening {
                    let read = &mut opening_formatting.read;
                    let formatting = opening_formatting.formatting;
                    read.push_str(&text);
                    if formatting.starts_with(read.as_str()) && read.len() < formatting.len() {
                        return vec![];
                    }
                    text = read.strip_prefix(formatting).unwrap_or(read).to_string();
                    *opening = None;
                }
                held.push_str(&text);
                let ready = held.len() - formatting_start(held, before);
                let pieces = text_piece(*kind, &held[..ready]);
                *held = held[ready..].to_string();
                pieces
            }
            State::ToolCalls { buffer } => {
                buffer.push_str(text);
                vec![]
            }
            State::Ended => vec![],
        }
    }

    /// Ends the stretch the response is in, and returns what it still holds.
    /// `formatted` is whether the marker ending it is the one whose formatting
    /// the stretch could be holding.
    fn close_stretch(&self, formatted: bool) -> Vec<Piece> {
        let before = self.formatting_before();
        match &self.state {
            State::Stretch {
                kind,
                opening,
                held,
            } => {
                // What couldn't be read as the formatting after the last marker
                // is text after all.
                let opened = opening.as_ref().map_or("", |o| o.read.as_str());
                let rest = format!("{opened}{held}");
                let text = if formatted {
                    rest.strip_suffix(before).unwrap_or(&rest)
                } else {
                    &rest
                };
                text_piece(*kind, text)
            }
            State::ToolCalls { buffer } => vec![self.read_tool_calls(buffer, false)],
            State::Ended => vec![],
        }
    }

    /// The formatting the template writes before the marker that ends the
    /// stretch the response is in.
    fn formatting_before(&self) -> &'static str {
        match self.state {
            State::Stretch {
                kind: Stretch::Text,
                ..
            } => self.format.format.tool_calls().before_begin,
            State::Stretch {
                kind: Stretch::Thinking,
                ..
            } => self.format.format.thinking().map_or("", |t| t.before_end),
            State::ToolCalls { .. } | State::Ended => "",
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

fn stretch(kind: Stretch, after_marker: &'static str) -> State {
    State::Stretch {
        kind,
        opening: (!after_marker.is_empty()).then(|| Opening {
            formatting: after_marker,
            read: String::new(),
        }),
        held: String::new(),
    }
}

fn text_piece(kind: Stretch, text: &str) -> Vec<Piece> {
    if text.is_empty() {
        return vec![];
    }
    let text = text.to_string();
    vec![match kind {
        Stretch::Text => Piece::Text(text),
        Stretch::Thinking => Piece::Thinking(text),
    }]
}

/// The length of the longest end of `text` that could be the start of
/// `formatting`.
fn formatting_start(text: &str, formatting: &str) -> usize {
    (1..=text.len().min(formatting.len()))
        .rev()
        .find(|&n| {
            let start = text.len() - n;
            text.is_char_boundary(start) && formatting.starts_with(&text[start..])
        })
        .unwrap_or(0)
}
