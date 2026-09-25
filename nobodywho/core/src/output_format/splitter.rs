use super::{ParseError, ResolvedFormat, Vocab};
use crate::tool_calling::Tool;
use encoding_rs::{Decoder, UTF_8};
use llama_cpp_2::token::LlamaToken;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// A generated token, and the text the output gained with it. A character
/// split across tokens is the text of the token that completes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    #[serde(with = "token_id")]
    pub id: LlamaToken,
    pub text: String,
}

/// A token as its plain id.
mod token_id {
    use llama_cpp_2::token::LlamaToken;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(token: &LlamaToken, serializer: S) -> Result<S::Ok, S::Error> {
        token.0.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<LlamaToken, D::Error> {
        Deserialize::deserialize(deserializer).map(LlamaToken)
    }
}

/// Part of a response, with the tokens it was written with. All pieces'
/// tokens, in order, are exactly the tokens generated.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub kind: PieceKind,
    pub tokens: Vec<Token>,
}

/// Items open, grow, and close in turn, never overlapping. Neither text nor
/// reasoning includes the formatting the template writes around markers.
#[derive(Clone, Debug, PartialEq)]
pub enum PieceKind {
    /// An item begins. Its tokens are its marker and the formatting around it,
    /// if it has any.
    Open(Item),
    /// More of the open item: text, reasoning, or a tool call's arguments as
    /// JSON.
    Delta(String),
    /// The open item ends. Its tokens are its marker and the formatting
    /// around it, if it has any.
    Close,
    /// Something the caller may want to log. It adds nothing to the output.
    Warning(Warning),
    /// The response ended, either by the model or cut off before that, e.g.
    /// by a token limit.
    End { cut_off: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Text,
    Thinking,
    ToolCall { name: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Warning {
    /// A marker that doesn't make sense in context, e.g. an end marker with
    /// nothing to end.
    Stray(&'static str),
    /// A block of tool calls that couldn't be read. Its text follows as text.
    Malformed(ParseError),
}

/// Splits a response into text, reasoning, and tool calls as it's generated.
pub struct Splitter<'a> {
    /// `None` when the model's output format isn't known, so that everything
    /// but the end of generation is text.
    format: Option<&'a ResolvedFormat>,
    end_of_generation: Vec<LlamaToken>,
    tools: &'a [Tool],
    state: State,
    /// Holds a character split across tokens until the rest of it arrives.
    decoder: Decoder,
    /// The item the pieces so far leave open.
    open: Option<Open>,
    /// Tokens not yet in a piece, each with where its text starts in the
    /// output.
    tokens: VecDeque<(usize, Token)>,
    /// The length of the output's text.
    written: usize,
    /// Where the output the pieces so far cover ends.
    covered: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Open {
    Text,
    Thinking,
    ToolCall,
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
        /// Where the block's text starts in the output.
        start: usize,
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
    /// The piece for the marker the formatting follows, which waits to cover
    /// the formatting too.
    marker: Option<PieceKind>,
}

impl<'a> Splitter<'a> {
    /// `thinking` is `Some` when the prompt left the response reasoning, with
    /// the formatting after the reasoning's `begin` that's still to come.
    pub(super) fn new(
        format: &'a ResolvedFormat,
        tools: &'a [Tool],
        thinking: Option<&'static str>,
    ) -> Self {
        let opening = thinking.map(|after_begin| Opening {
            formatting: after_begin,
            read: String::new(),
            marker: Some(PieceKind::Open(Item::Thinking)),
        });
        let kind = match opening {
            Some(_) => Stretch::Thinking,
            None => Stretch::Text,
        };
        Splitter::start(
            Some(format),
            format.end_of_generation.clone(),
            tools,
            kind,
            opening,
        )
    }

    /// A splitter for a model whose output format isn't known, so that
    /// everything but the end of generation is text.
    pub fn plain(vocab: &impl Vocab) -> Splitter<'static> {
        Splitter::start(None, vocab.end_of_generation(), &[], Stretch::Text, None)
    }

    fn start(
        format: Option<&'a ResolvedFormat>,
        end_of_generation: Vec<LlamaToken>,
        tools: &'a [Tool],
        kind: Stretch,
        opening: Option<Opening>,
    ) -> Self {
        Splitter {
            format,
            end_of_generation,
            tools,
            state: State::Stretch {
                kind,
                opening,
                held: String::new(),
            },
            decoder: UTF_8.new_decoder_without_bom_handling(),
            open: None,
            tokens: VecDeque::new(),
            written: 0,
            covered: 0,
        }
    }

    /// Takes the next token and the bytes it decodes to, and returns the
    /// pieces it finishes. There's nothing after the end of generation, so a
    /// token pushed then panics in debug builds and is ignored otherwise.
    pub fn push(&mut self, token: LlamaToken, bytes: &[u8]) -> Vec<Piece> {
        let is_end_of_generation = self.end_of_generation.contains(&token);
        let format = self.format;
        let is_tool_call_begin = format.is_some_and(|f| token == f.tool_calls.begin);
        let is_tool_call_end = format.is_some_and(|f| Some(token) == f.tool_calls.end);
        let thinking = format.and_then(|f| f.thinking);
        let is_thinking_begin = thinking.is_some_and(|t| token == t.begin);
        let is_thinking_end = thinking.is_some_and(|t| token == t.end);

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
            self.tokens.push_back((
                self.written,
                Token {
                    id: token,
                    text: text.clone(),
                },
            ));
            self.written += text.len();
            return self.add_text(&text);
        }

        // A marker ends the text before it, along with any character that
        // text left unfinished.
        let unfinished = self.decode(&[], true);
        let marker = String::from_utf8_lossy(bytes);
        self.tokens.push_back((
            self.written,
            Token {
                id: token,
                text: format!("{unfinished}{marker}"),
            },
        ));
        self.written += unfinished.len();
        let mut pieces = self.add_text(&unfinished);
        let end = self.written;
        self.written += marker.len();

        if is_end_of_generation {
            pieces.extend(self.end(end, false));
            return pieces;
        }
        let format = format.expect("without a format, only the end of generation is a marker");
        let tool_calls = format.format.tool_calls();
        let thinking = format.format.thinking();
        match &self.state {
            State::Stretch {
                kind: Stretch::Text,
                ..
            } if is_tool_call_begin => {
                // The text stays open, since the block might not be calls.
                pieces.extend(self.flush(end, true));
                self.state = State::ToolCalls {
                    buffer: String::new(),
                    start: self.written,
                };
            }
            State::Stretch {
                kind: Stretch::Text,
                ..
            } if is_thinking_begin => {
                pieces.extend(self.flush(end, false));
                pieces.extend(self.close_open());
                let after_begin = thinking.map_or("", |t| t.after_begin);
                pieces.extend(self.begin_stretch(
                    Stretch::Thinking,
                    after_begin,
                    Some(PieceKind::Open(Item::Thinking)),
                ));
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
                let marker = marker.expect("only markers the format has match");
                pieces.extend(self.flush(end, false));
                let warning = PieceKind::Warning(Warning::Stray(marker));
                pieces.push(self.piece(warning, self.written));
            }
            State::Stretch {
                kind: Stretch::Thinking,
                ..
            } => {
                pieces.extend(self.flush(end, true));
                let after_end = thinking.map_or("", |t| t.after_end);
                pieces.extend(self.begin_stretch(Stretch::Text, after_end, Some(PieceKind::Close)));
            }
            // Both end the block, but only `end` is part of it. A block with
            // no end runs until the next one begins.
            State::ToolCalls { buffer, start } => {
                let (buffer, start) = (buffer.clone(), *start);
                if is_tool_call_end {
                    let (read, close) = self.read_tool_calls(&buffer, start, self.written, true);
                    pieces.extend(read);
                    pieces.extend(self.begin_stretch(Stretch::Text, tool_calls.after_end, close));
                } else {
                    let (read, close) = self.read_tool_calls(&buffer, start, end, false);
                    pieces.extend(read);
                    pieces.extend(close.map(|close| self.piece(close, end)));
                    self.state = State::ToolCalls {
                        buffer: String::new(),
                        start: self.written,
                    };
                }
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
    /// of tool calls it left open. Does nothing to one that has ended.
    pub fn finish(&mut self) -> Vec<Piece> {
        if matches!(self.state, State::Ended) {
            return vec![];
        }
        // The token that began an unfinished character is still waiting for
        // text, having had none.
        let unfinished = self.decode(&[], true);
        if let Some((_, token)) = self.tokens.back_mut() {
            token.text.push_str(&unfinished);
        }
        self.written += unfinished.len();
        let mut pieces = self.add_text(&unfinished);
        pieces.extend(self.end(self.written, true));
        pieces
    }

    /// Decodes a token's bytes. `last` gives up on a character left
    /// unfinished, and readies the decoder for new text.
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

    /// A piece covering the output up to `end`, with every token whose text
    /// starts before that.
    fn piece(&mut self, kind: PieceKind, end: usize) -> Piece {
        debug_assert!(end >= self.covered, "pieces cover the output in order");
        let mut tokens = Vec::new();
        while self.tokens.front().is_some_and(|(start, _)| *start < end) {
            let (_, token) = self.tokens.pop_front().expect("checked above");
            tokens.push(token);
        }
        self.covered = end;
        match &kind {
            PieceKind::Open(Item::Text) => self.open = Some(Open::Text),
            PieceKind::Open(Item::Thinking) => self.open = Some(Open::Thinking),
            PieceKind::Open(Item::ToolCall { .. }) => self.open = Some(Open::ToolCall),
            PieceKind::Close => self.open = None,
            PieceKind::Delta(_) | PieceKind::Warning(_) | PieceKind::End { .. } => {}
        }
        Piece { kind, tokens }
    }

    /// Adds text, which ends the output, to where the response is, and returns
    /// the pieces that are now certain.
    fn add_text(&mut self, text: &str) -> Vec<Piece> {
        let before = self.formatting_before();
        let (kind, opening, held) = match &mut self.state {
            State::Stretch {
                kind,
                opening,
                held,
            } => (*kind, opening, held),
            State::ToolCalls { buffer, .. } => {
                buffer.push_str(text);
                return vec![];
            }
            State::Ended => return vec![],
        };

        let mut text = text.to_string();
        let mut marker = None;
        if let Some(Opening {
            formatting,
            read,
            marker: waiting,
        }) = opening
        {
            read.push_str(&text);
            if formatting.starts_with(read.as_str()) && read.len() < formatting.len() {
                return vec![];
            }
            text = read.strip_prefix(*formatting).unwrap_or(read).to_string();
            marker = waiting.take().map(|waiting| (waiting, text.len()));
            *opening = None;
        }
        held.push_str(&text);
        let ready = held.len() - formatting_start(held, before);
        let content = held[..ready].to_string();
        *held = held[ready..].to_string();
        let held = held.len();

        let mut pieces = Vec::new();
        if let Some((marker, after)) = marker {
            pieces.push(self.piece(marker, self.written - after));
        }
        pieces.extend(self.content(kind, &content, self.written - held));
        pieces
    }

    /// Ends the stretch's text at `end`, where a marker comes. `formatted` is
    /// whether it's the marker whose formatting the stretch could be holding.
    fn flush(&mut self, end: usize, formatted: bool) -> Vec<Piece> {
        let before = self.formatting_before();
        let State::Stretch {
            kind,
            opening,
            held,
        } = &mut self.state
        else {
            return vec![];
        };
        let kind = *kind;
        let opening = opening.take();
        // What couldn't be read as the formatting after the last marker is
        // text after all.
        let read = opening.as_ref().map_or("", |o| o.read.as_str());
        let rest = format!("{read}{held}");
        held.clear();
        let start = end - rest.len();

        let mut pieces = Vec::new();
        if let Some(marker) = opening.and_then(|o| o.marker) {
            pieces.push(self.piece(marker, start));
        }
        let text = if formatted {
            rest.strip_suffix(before).unwrap_or(&rest)
        } else {
            &rest
        };
        pieces.extend(self.content(kind, text, start + text.len()));
        pieces
    }

    /// Text or reasoning ending at `end`, opening a text item if one isn't.
    fn content(&mut self, kind: Stretch, text: &str, end: usize) -> Vec<Piece> {
        if text.is_empty() {
            return vec![];
        }
        let mut pieces = Vec::new();
        match kind {
            Stretch::Text if self.open != Some(Open::Text) => {
                pieces.push(self.piece(PieceKind::Open(Item::Text), end - text.len()));
            }
            Stretch::Text => {}
            Stretch::Thinking => debug_assert_eq!(self.open, Some(Open::Thinking)),
        }
        pieces.push(self.piece(PieceKind::Delta(text.to_string()), end));
        pieces
    }

    fn close_open(&mut self) -> Vec<Piece> {
        match self.open {
            Some(_) => vec![self.piece(PieceKind::Close, self.covered)],
            None => vec![],
        }
    }

    /// Starts a stretch after a marker, whose piece covers the formatting
    /// after it once that's clear.
    fn begin_stretch(
        &mut self,
        kind: Stretch,
        after_marker: &'static str,
        marker: Option<PieceKind>,
    ) -> Vec<Piece> {
        let mut pieces = Vec::new();
        let opening = if after_marker.is_empty() {
            pieces.extend(marker.map(|marker| self.piece(marker, self.written)));
            None
        } else {
            Some(Opening {
                formatting: after_marker,
                read: String::new(),
                marker,
            })
        };
        self.state = State::Stretch {
            kind,
            opening,
            held: String::new(),
        };
        pieces
    }

    /// Ends the response, whose output before its last marker, if any, ends
    /// at `end`.
    fn end(&mut self, end: usize, cut_off: bool) -> Vec<Piece> {
        let mut pieces = match &self.state {
            State::Stretch { .. } => self.flush(end, false),
            State::ToolCalls { buffer, start } => {
                let (buffer, start) = (buffer.clone(), *start);
                let (mut read, close) = self.read_tool_calls(&buffer, start, end, false);
                read.extend(close.map(|close| self.piece(close, end)));
                read
            }
            State::Ended => vec![],
        };
        pieces.extend(self.close_open());
        pieces.push(self.piece(PieceKind::End { cut_off }, self.written));
        debug_assert!(self.tokens.is_empty(), "every token is in a piece");
        self.state = State::Ended;
        pieces
    }

    /// The formatting the template writes before the marker that ends the
    /// stretch the response is in.
    fn formatting_before(&self) -> &'static str {
        let Some(format) = self.format else {
            return "";
        };
        match self.state {
            State::Stretch {
                kind: Stretch::Text,
                ..
            } => format.format.tool_calls().before_begin,
            State::Stretch {
                kind: Stretch::Thinking,
                ..
            } => format.format.thinking().map_or("", |t| t.before_end),
            State::ToolCalls { .. } | State::Ended => "",
        }
    }

    /// Reads a block of tool calls whose text starts at `start`, and which
    /// ends at `end`. `closed` is whether it got its end marker. Returns the
    /// pieces, and the last call's `Close` if it has one, which the caller
    /// places since it covers what follows the block.
    fn read_tool_calls(
        &mut self,
        text: &str,
        start: usize,
        end: usize,
        closed: bool,
    ) -> (Vec<Piece>, Option<PieceKind>) {
        let format = self.format.expect("only an output format finds tool calls");
        let mut pieces = Vec::new();
        match format.parse_tool_call_spans(text, self.tools) {
            Ok(calls) => {
                pieces.extend(self.close_open());
                let last = calls.len() - 1;
                for (i, (call, span)) in calls.into_iter().enumerate() {
                    let open = PieceKind::Open(Item::ToolCall { name: call.name });
                    pieces.push(self.piece(open, start + span.start));
                    let arguments = PieceKind::Delta(call.arguments.to_string());
                    pieces.push(self.piece(arguments, start + span.end));
                    if i < last {
                        pieces.push(self.piece(PieceKind::Close, start + span.end));
                    }
                }
                (pieces, Some(PieceKind::Close))
            }
            Err(error) => {
                let warning = PieceKind::Warning(Warning::Malformed(error));
                pieces.push(self.piece(warning, self.covered));
                let syntax = format.format.tool_calls();
                let end_marker = syntax.end.filter(|_| closed).unwrap_or("");
                let text = format!("{}{text}{end_marker}", syntax.begin);
                pieces.extend(self.content(Stretch::Text, &text, end));
                (pieces, None)
            }
        }
    }
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
