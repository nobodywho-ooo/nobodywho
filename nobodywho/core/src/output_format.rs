//! How models write their output, each described once and used both to
//! constrain generation and to read it.

mod formats;
mod grammar;
mod parse;
mod splitter;

pub use formats::{FunctionGemma, Gemma4, Lfm2, Ministral3, Qwen3, Qwen35};
pub use parse::ParseError;
pub use splitter::{Piece, Splitter};

use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::token_type::LlamaTokenAttr;
use std::collections::HashMap;
use std::fmt::Debug;
use tracing::debug;

/// How a model family writes its output: its tool calls, and its reasoning if
/// it has any. The end of generation comes from the vocabulary instead, which
/// records it for every model.
///
/// The strings are copied from the model's chat template. A marker the
/// vocabulary has as a control token must be a whole string here, since control
/// tokens have no text to match partway through.
pub trait OutputFormat: Debug + Sync {
    fn tool_calls(&self) -> ToolCallSyntax;

    /// `None` for models that don't reason.
    fn thinking(&self) -> Option<ThinkingSyntax> {
        None
    }

    /// Whether a chat template renders tool calls in this format.
    fn detect(&self, template: &str) -> bool {
        let syntax = self.tool_calls();
        template.contains(syntax.begin) || syntax.end.is_some_and(|end| template.contains(end))
    }
}

/// A block of tool calls, written `begin CALLS end`.
#[derive(Clone, Copy, Debug)]
pub struct ToolCallSyntax {
    /// Must be a single special token.
    pub begin: &'static str,
    /// Must be a single special token. `None` when a block runs to the next
    /// `begin` or the end of the output.
    pub end: Option<&'static str>,
    /// Set when one block holds several calls, as in `[a(), b()]`. Otherwise
    /// each call gets a block of its own.
    pub list: Option<ListSyntax>,
    pub call: CallSyntax,
}

/// Reasoning, written `begin label REASONING end`.
#[derive(Clone, Copy, Debug)]
pub struct ThinkingSyntax {
    /// Must be a single special token, like `end`. A model whose vocabulary
    /// lacks either is taken not to reason.
    pub begin: &'static str,
    /// Text that follows `begin` and isn't part of the reasoning, like the
    /// channel name after Gemma4's `<|channel>`.
    pub label: &'static str,
    pub end: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub enum CallSyntax {
    Parts(CallParts),
    /// One JSON object holding the name and the arguments under these keys,
    /// read in either order.
    JsonObject {
        name_key: &'static str,
        arguments_key: &'static str,
    },
}

/// One call, written `before_name NAME after_name ARGUMENTS after_args`.
#[derive(Clone, Copy, Debug)]
pub struct CallParts {
    pub before_name: &'static str,
    pub after_name: &'static str,
    pub args: ArgsSyntax,
    pub after_args: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub enum ArgsSyntax {
    /// All arguments as one JSON object.
    Json,
    /// Each argument written `before_key KEY after_key VALUE after_value`, with
    /// `separator` between arguments.
    KeyValue {
        before_key: &'static str,
        after_key: &'static str,
        after_value: &'static str,
        separator: &'static str,
        value: ValueSyntax,
    },
}

/// How a single argument's value is written.
#[derive(Clone, Copy, Debug)]
pub enum ValueSyntax {
    /// JSON. Python's `True`, `False`, `None` and single-quoted strings are read
    /// too, but never generated.
    Json,
    /// Strings as they are, anything else as JSON. The value runs up to the
    /// argument's `after_value`, which has to be text.
    Raw,
    /// Every value as text between two copies of a marker, whatever its type.
    Delimited(&'static str),
    /// JSON's shape, but with strings unescaped between two copies of `quote`,
    /// and object keys bare.
    JsonLike { quote: &'static str },
}

/// Several calls in one block, written `open CALL separator CALL ... close`.
#[derive(Clone, Copy, Debug)]
pub struct ListSyntax {
    pub open: &'static str,
    pub separator: &'static str,
    pub close: &'static str,
}

/// Every format, in the order `detect` tries them. A format whose markers are
/// a superset of another's has to come first.
pub const FORMATS: &[&dyn OutputFormat] =
    &[&FunctionGemma, &Gemma4, &Qwen35, &Qwen3, &Ministral3, &Lfm2];

#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("{format:?} expects {marker:?} to be a single special token, but the vocabulary doesn't have one")]
    NotSpecial {
        format: &'static dyn OutputFormat,
        marker: &'static str,
    },

    #[error(
        "{format:?} needs {marker:?} to be text, but the vocabulary has it as a control token"
    )]
    NotText {
        format: &'static dyn OutputFormat,
        marker: &'static str,
    },

    #[error("a tool call grammar needs at least one tool")]
    NoTools,

    #[error("failed to generate grammar: {0}")]
    Grammar(String),

    #[error("no known tool call format matches this model")]
    Undetected,

    #[error("failed to read the model's chat template: {0}")]
    ChatTemplate(#[from] llama_cpp_2::ChatTemplateError),
}

/// A vocabulary entry a marker can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpecialToken {
    pub id: LlamaToken,
    /// Control tokens have no text, so a grammar has to name them by id.
    pub control: bool,
}

pub trait Vocab {
    /// The special token spelled exactly `text`, if the vocabulary has one.
    fn special_token(&self, text: &str) -> Option<SpecialToken>;

    /// The tokens that end generation.
    fn end_of_generation(&self) -> Vec<LlamaToken>;
}

impl Vocab for LlamaModel {
    fn special_token(&self, text: &str) -> Option<SpecialToken> {
        let tokens = self.str_to_token(text, AddBos::Never).ok()?;
        let &[id] = tokens.as_slice() else {
            return None;
        };
        let attrs = self.token_attr(id).0;
        let control = attrs.contains(LlamaTokenAttr::Control);
        (control || attrs.contains(LlamaTokenAttr::UserDefined))
            .then_some(SpecialToken { id, control })
    }

    fn end_of_generation(&self) -> Vec<LlamaToken> {
        (0..self.n_vocab())
            .map(LlamaToken)
            .filter(|&token| self.is_eog_token(token))
            .collect()
    }
}

/// A format checked against a model's vocabulary, ready to constrain
/// generation and to read it.
#[derive(Clone, Debug)]
pub struct ResolvedFormat {
    format: &'static dyn OutputFormat,
    tool_calls: ToolCallTokens,
    /// `None` if the model doesn't reason.
    thinking: Option<ThinkingTokens>,
    end_of_generation: Vec<LlamaToken>,
    /// The tool call markers that the vocabulary has as control tokens.
    control: HashMap<&'static str, LlamaToken>,
}

/// The tokens of `ToolCallSyntax`'s markers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ToolCallTokens {
    begin: LlamaToken,
    end: Option<LlamaToken>,
}

/// The tokens of `ThinkingSyntax`'s markers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ThinkingTokens {
    begin: LlamaToken,
    end: LlamaToken,
}

impl ResolvedFormat {
    pub fn new(format: &'static dyn OutputFormat, vocab: &impl Vocab) -> Result<Self, FormatError> {
        let syntax = format.tool_calls();
        let special = |marker: &'static str| {
            vocab
                .special_token(marker)
                .ok_or(FormatError::NotSpecial { format, marker })
        };
        let tool_calls = ToolCallTokens {
            begin: special(syntax.begin)?.id,
            end: syntax.end.map(special).transpose()?.map(|t| t.id),
        };

        let thinking = format.thinking().and_then(|thinking| {
            let begin = vocab.special_token(thinking.begin);
            let end = vocab.special_token(thinking.end);
            if begin.is_none() || end.is_none() {
                debug!(
                    ?format,
                    ?thinking,
                    "The vocabulary lacks the thinking markers, so the model doesn't reason"
                );
            }
            Some(ThinkingTokens {
                begin: begin?.id,
                end: end?.id,
            })
        });

        let control: HashMap<_, _> = markers(&syntax)
            .into_iter()
            .filter_map(|marker| {
                let token = vocab.special_token(marker)?;
                token.control.then_some((marker, token.id))
            })
            .collect();

        // A lazily matched value can only stop at text.
        if let Some(marker) = text_markers(&syntax)
            .into_iter()
            .find(|marker| control.contains_key(marker))
        {
            return Err(FormatError::NotText { format, marker });
        }

        Ok(ResolvedFormat {
            format,
            tool_calls,
            thinking,
            end_of_generation: vocab.end_of_generation(),
            control,
        })
    }

    pub fn format(&self) -> &'static dyn OutputFormat {
        self.format
    }

    /// A splitter for the response to `prompt`, the rendered template it
    /// continues. `tools` is only used to read argument values by their schema
    /// type.
    pub fn splitter<'a>(
        &'a self,
        tools: &'a [crate::tool_calling::Tool],
        prompt: &str,
    ) -> Splitter<'a> {
        Splitter::new(self, tools, self.opens_thinking(prompt))
    }

    /// Whether `prompt` leaves the response already reasoning, as templates do
    /// that open the reasoning for the model.
    fn opens_thinking(&self, prompt: &str) -> bool {
        let Some(thinking) = self.format.thinking().filter(|_| self.thinking.is_some()) else {
            return false;
        };
        match (prompt.rfind(thinking.begin), prompt.rfind(thinking.end)) {
            (Some(begin), Some(end)) => begin > end,
            (begin, _) => begin.is_some(),
        }
    }
}

/// Every string tool calls use, so each can be checked against the vocabulary.
fn markers(syntax: &ToolCallSyntax) -> Vec<&'static str> {
    let mut markers = vec![syntax.begin];
    markers.extend(syntax.end);
    if let Some(list) = syntax.list {
        markers.extend([list.open, list.separator, list.close]);
    }
    let CallSyntax::Parts(call) = syntax.call else {
        return markers;
    };
    markers.extend([call.before_name, call.after_name, call.after_args]);
    if let ArgsSyntax::KeyValue {
        before_key,
        after_key,
        after_value,
        separator,
        value,
    } = call.args
    {
        markers.extend([before_key, after_key, after_value, separator]);
        match value {
            ValueSyntax::Delimited(quote) | ValueSyntax::JsonLike { quote } => markers.push(quote),
            ValueSyntax::Json | ValueSyntax::Raw => {}
        }
    }
    markers.retain(|marker| !marker.is_empty());
    markers
}

/// The markers that end a lazily matched value.
fn text_markers(syntax: &ToolCallSyntax) -> Vec<&'static str> {
    match syntax.call {
        CallSyntax::Parts(CallParts {
            args:
                ArgsSyntax::KeyValue {
                    value: ValueSyntax::Raw,
                    after_value,
                    ..
                },
            ..
        }) => vec![after_value],
        _ => vec![],
    }
}

/// The format a model writes tool calls in, from its chat template or failing
/// that its metadata.
pub fn detect(model: &LlamaModel) -> Result<&'static dyn OutputFormat, FormatError> {
    let template = model
        .chat_template(Some("tool_use"))
        .and_then(|t| Ok(t.to_string()?))
        .or_else(|_| model.chat_template(None).and_then(|t| Ok(t.to_string()?)))?;

    if let Some(&format) = FORMATS.iter().find(|format| format.detect(&template)) {
        debug!(?format, "Detected tool call format from chat template");
        return Ok(format);
    }

    let arch = model
        .meta_val_str("general.architecture")
        .unwrap_or_default()
        .to_lowercase();
    let name = model
        .meta_val_str("general.name")
        .unwrap_or_default()
        .to_lowercase();
    let format: &'static dyn OutputFormat = if is_qwen35_36(&arch) {
        &Qwen35
    } else if arch.starts_with("qwen3") {
        &Qwen3
    } else if arch.starts_with("lfm") || name.contains("lfm") {
        &Lfm2
    } else if name.contains("functiongemma") || name.contains("function-gemma") {
        &FunctionGemma
    } else if name.contains("gemma-4") || name.contains("gemma4") {
        &Gemma4
    } else if is_qwen35_36(&name) {
        &Qwen35
    } else if name.contains("qwen") {
        &Qwen3
    } else {
        return Err(FormatError::Undetected);
    };
    debug!(?format, %arch, %name, "Detected tool call format from model metadata");
    Ok(format)
}

fn is_qwen35_36(s: &str) -> bool {
    [
        "qwen35", "qwen36", "qwen3.5", "qwen3.6", "qwen 3.5", "qwen 3.6", "qwen-3.5", "qwen-3.6",
    ]
    .iter()
    .any(|needle| s.contains(needle))
}

#[cfg(test)]
mod tests;
