//! How models write their output, each described once and used both to
//! constrain generation and to read it.

mod formats;
mod grammar;
mod parse;
mod splitter;

pub use formats::{function_gemma, gemma4, lfm2, ministral3, qwen3, qwen35};
pub use parse::ParseError;
pub use splitter::{Item, Piece, PieceKind, Splitter, Token, Warning};

use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::token_type::LlamaTokenAttr;
use std::collections::HashMap;
use tracing::debug;

/// How a model family writes its output: its tool calls, and its reasoning if
/// it has any. The end of generation comes from the vocabulary instead, which
/// records it for every model.
#[derive(Clone, Copy, Debug)]
pub struct OutputFormat {
    /// For logs and errors.
    pub name: &'static str,
    pub tool_calls: ToolCallSyntax,
    /// `None` for models that don't reason.
    pub thinking: Option<ThinkingSyntax>,
}

/// A block of tool calls, written `before_begin begin CALLS end after_end`.
#[derive(Clone, Copy, Debug)]
pub struct ToolCallSyntax {
    pub before_begin: &'static str,
    /// Must be a single special token.
    pub begin: &'static str,
    /// Must be a single special token. `None` when a block runs to the next
    /// `begin` or the end of the output.
    pub end: Option<&'static str>,
    pub after_end: &'static str,
    /// Set when one block holds several calls. Otherwise
    /// each call gets a block of its own.
    pub list: Option<ListSyntax>,
    pub several_blocks_allowed: bool,
    pub call: CallSyntax,
}

/// Reasoning, written `begin after_begin REASONING before_end end after_end`.
///
/// `after_begin`, `before_end` and `after_end` are formatting the template
/// writes around the markers, which is neither reasoning nor text. That
/// includes a label like the channel name after Gemma4's `<|channel>`.
#[derive(Clone, Copy, Debug)]
pub struct ThinkingSyntax {
    pub begin: &'static str,
    pub after_begin: &'static str,
    pub before_end: &'static str,
    pub end: &'static str,
    pub after_end: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub enum CallSyntax {
    Parts(CallParts),
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
    Json,
    Raw,
    /// Every value as text between two copies of a marker, whatever its type.
    Delimited(&'static str),
    /// JSON's shape, but with strings unescaped between two copies of `quote`,
    /// and object keys bare.
    JsonLike {
        quote: &'static str,
    },
}

/// Several calls in one block, written `open CALL separator CALL ... close`.
#[derive(Clone, Copy, Debug)]
pub struct ListSyntax {
    pub open: &'static str,
    pub separator: &'static str,
    pub close: &'static str,
}

#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("{format_name} expects {marker:?} to be a single special token, but the vocabulary doesn't have one")]
    NotSpecial {
        format_name: &'static str,
        marker: &'static str,
    },

    #[error(
        "{format_name} needs {marker:?} to be text, but the vocabulary has it as a control token"
    )]
    NotText {
        format_name: &'static str,
        marker: &'static str,
    },

    #[error("a tool call grammar needs at least one tool")]
    NoTools,

    #[error("no known tool call format matches this model")]
    Undetected,

    #[error("failed to read the model's chat template: {0}")]
    ChatTemplate(#[from] llama_cpp_2::ChatTemplateError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpecialToken {
    pub id: LlamaToken,
    pub control: bool,
}

pub trait Vocab {
    /// The special token spelled exactly `text`, if the vocabulary has one.
    fn special_token(&self, text: &str) -> Option<SpecialToken>;

    /// The tokens that each end generation.
    fn end_of_generation(&self) -> Vec<LlamaToken>;
}

impl Vocab for LlamaModel {
    fn special_token(&self, text: &str) -> Option<SpecialToken> {
        let tokens = self.vocab().tokenize(text.as_bytes(), false, true);
        let &[id] = tokens.as_slice() else {
            return None;
        };
        let attrs = self.vocab().attr(id).0;
        let control = attrs.contains(LlamaTokenAttr::Control);
        (control || attrs.contains(LlamaTokenAttr::UserDefined))
            .then_some(SpecialToken { id, control })
    }

    fn end_of_generation(&self) -> Vec<LlamaToken> {
        (0..self.n_vocab())
            .map(LlamaToken)
            .filter(|&token| self.vocab().is_eog(token))
            .collect()
    }
}

/// A format with the information needed from the vocabulary for parsing.
#[derive(Clone, Debug)]
pub struct ResolvedFormat {
    format: OutputFormat,
    tool_calls: ToolCallTokens,
    thinking: Option<ThinkingTokens>,
    end_of_generation: Vec<LlamaToken>,
    control: HashMap<&'static str, LlamaToken>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ToolCallTokens {
    begin: LlamaToken,
    end: Option<LlamaToken>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ThinkingTokens {
    begin: LlamaToken,
    end: LlamaToken,
}

impl ResolvedFormat {
    pub fn new(format: OutputFormat, vocab: &impl Vocab) -> Result<Self, FormatError> {
        let syntax = format.tool_calls;
        let special = |marker: &'static str| {
            vocab.special_token(marker).ok_or(FormatError::NotSpecial {
                format_name: format.name,
                marker,
            })
        };
        let tool_calls = ToolCallTokens {
            begin: special(syntax.begin)?.id,
            end: syntax.end.map(special).transpose()?.map(|t| t.id),
        };

        let thinking = format.thinking.and_then(|thinking| {
            let begin = vocab.special_token(thinking.begin);
            let end = vocab.special_token(thinking.end);
            if begin.is_none() || end.is_none() {
                debug!(
                    format = format.name,
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
        if let Some(marker) = text_markers(&syntax)
            .into_iter()
            .find(|marker| control.contains_key(marker))
        {
            return Err(FormatError::NotText {
                format_name: format.name,
                marker,
            });
        }

        Ok(ResolvedFormat {
            format,
            tool_calls,
            thinking,
            end_of_generation: vocab.end_of_generation(),
            control,
        })
    }

    pub fn format(&self) -> &OutputFormat {
        &self.format
    }

    /// A splitter that parses model output according to this format.
    pub fn splitter(self, tools: Vec<crate::tool_calling::Tool>, prompt: &str) -> Splitter {
        let opens_thinking = self.opens_thinking(prompt);
        Splitter::new(self, tools, opens_thinking)
    }

    /// Whether `prompt` ends with an open reasoning block. If so, gives the prompt from
    /// `begin` on, and the formatting after `begin` that it hasn't written yet.
    fn opens_thinking(&self, prompt: &str) -> Option<(String, &'static str)> {
        let thinking = self.format.thinking.filter(|_| self.thinking.is_some())?;
        let begin = prompt.rfind(thinking.begin)?;

        let written = &prompt[begin + thinking.begin.len()..];
        let formatting = thinking.after_begin.strip_prefix(written)?;
        Some((prompt[begin..].to_string(), formatting))
    }
}

#[derive(Clone, Debug)]
pub enum ModelOutput {
    Formatted(Box<ResolvedFormat>),
    Plain { end_of_generation: Vec<LlamaToken> },
}

impl ModelOutput {
    pub fn plain(vocab: &impl Vocab) -> Self {
        ModelOutput::Plain {
            end_of_generation: vocab.end_of_generation(),
        }
    }

    pub fn resolved_format(&self) -> Option<&ResolvedFormat> {
        match self {
            ModelOutput::Formatted(format) => Some(format),
            ModelOutput::Plain { .. } => None,
        }
    }

    /// A splitter that parses model output according to this format.
    pub fn splitter(self, tools: Vec<crate::tool_calling::Tool>, prompt: &str) -> Splitter {
        match self {
            ModelOutput::Formatted(format) => (*format).splitter(tools, prompt),
            ModelOutput::Plain { end_of_generation } => Splitter::plain(end_of_generation.clone()),
        }
    }
}

/// All strings used by the tool call syntax.
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
pub fn detect(model: &LlamaModel) -> Result<OutputFormat, FormatError> {
    let template = model
        .chat_template(Some("tool_use"))
        .and_then(|t| Ok(t.to_string()?))
        .or_else(|_| model.chat_template(None).and_then(|t| Ok(t.to_string()?)))?;

    if let Some(format) = detect_from_template(&template) {
        debug!(
            format = format.name,
            "Detected tool call format from chat template"
        );
        return Ok(format);
    }

    let arch = model
        .meta_val_str("general.architecture")
        .unwrap_or_default();
    let name = model.meta_val_str("general.name").unwrap_or_default();
    let format = detect_from_metadata(&arch, &name).ok_or(FormatError::Undetected)?;
    debug!(format = format.name, %arch, %name, "Detected tool call format from model metadata");
    Ok(format)
}

/// The format a chat template renders tool calls in.
fn detect_from_template(template: &str) -> Option<OutputFormat> {
    let has = |marker| template.contains(marker);
    if has("<start_function_call>") || has("<end_function_call>") {
        return Some(function_gemma());
    }
    // Before Qwen, since both contain `tool_call`.
    if has("<|tool_call>") || has("<tool_call|>") {
        return Some(gemma4());
    }
    if has("<tool_call>") || has("</tool_call>") {
        return Some(if has("<function=") { qwen35() } else { qwen3() });
    }
    if has("[TOOL_CALLS]") {
        return Some(ministral3());
    }
    if has("<|tool_call_start|>") || has("<|tool_list_start|>") || has("<|tool_response_start|>") {
        return Some(lfm2());
    }
    None
}

/// The format a model's `general.architecture` or `general.name` suggests.
fn detect_from_metadata(arch: &str, name: &str) -> Option<OutputFormat> {
    let arch_lower = arch.to_lowercase();
    if is_qwen35_36_architecture(&arch_lower) {
        return Some(qwen35());
    }
    if arch_lower.starts_with("qwen3") {
        return Some(qwen3());
    }
    if arch_lower.starts_with("lfm") {
        return Some(lfm2());
    }

    let name_lower = name.to_lowercase();
    if name_lower.contains("lfm") {
        return Some(lfm2());
    }
    if name_lower.contains("functiongemma") || name_lower.contains("function-gemma") {
        return Some(function_gemma());
    }
    if name_lower.contains("gemma-4") || name_lower.contains("gemma4") {
        return Some(gemma4());
    }
    if is_qwen35_36_name(&name_lower) {
        return Some(qwen35());
    }
    if is_qwen3_name(&name_lower) || name_lower.contains("qwen") {
        return Some(qwen3());
    }
    None
}

fn is_qwen35_36_architecture(arch: &str) -> bool {
    let arch = arch.to_lowercase();
    arch.starts_with("qwen35")
        || arch.starts_with("qwen36")
        || arch.contains("qwen3.5")
        || arch.contains("qwen3.6")
}

fn is_qwen35_36_name(name: &str) -> bool {
    let name = name.to_lowercase();
    [
        "qwen3.5", "qwen3.6", "qwen 3.5", "qwen 3.6", "qwen-3.5", "qwen-3.6", "qwen35", "qwen36",
    ]
    .iter()
    .any(|needle| name.contains(needle))
}

fn is_qwen3_name(name: &str) -> bool {
    let name = name.to_lowercase();
    name.contains("qwen3") || name.contains("qwen 3") || name.contains("qwen-3")
}

#[cfg(test)]
mod tests;
