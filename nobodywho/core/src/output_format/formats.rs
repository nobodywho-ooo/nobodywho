use super::{
    ArgsSyntax, CallParts, CallSyntax, ListSyntax, OutputFormat, ThinkingSyntax, ToolCallSyntax,
    ValueSyntax,
};

/// As Qwen's templates write it: `<think>\nREASONING\n</think>\n\n`.
const QWEN_THINKING: ThinkingSyntax = ThinkingSyntax {
    begin: "<think>",
    after_begin: "\n",
    before_end: "\n",
    end: "</think>",
    after_end: "\n\n",
};

/// `<tool_call>\n{"name": "f", "arguments": {"k": "v"}}\n</tool_call>`
#[derive(Debug)]
pub struct Qwen3;

impl OutputFormat for Qwen3 {
    fn tool_calls(&self) -> ToolCallSyntax {
        ToolCallSyntax {
            before_begin: "\n",
            begin: "<tool_call>",
            end: Some("</tool_call>"),
            after_end: "",
            list: None,
            call: CallSyntax::JsonObject {
                name_key: "name",
                arguments_key: "arguments",
            },
        }
    }

    fn thinking(&self) -> Option<ThinkingSyntax> {
        Some(QWEN_THINKING)
    }
}

/// `<tool_call>\n<function=f>\n<parameter=k>\nv\n</parameter>\n</function>\n</tool_call>`
#[derive(Debug)]
pub struct Qwen35;

impl OutputFormat for Qwen35 {
    fn tool_calls(&self) -> ToolCallSyntax {
        // `\n\n` after text, but only `\n` between calls.
        ToolCallSyntax {
            before_begin: "\n\n",
            begin: "<tool_call>",
            end: Some("</tool_call>"),
            after_end: "\n",
            list: None,
            call: CallSyntax::Parts(CallParts {
                before_name: "\n<function=",
                after_name: ">\n",
                args: ArgsSyntax::KeyValue {
                    before_key: "<parameter=",
                    after_key: ">\n",
                    after_value: "\n</parameter>\n",
                    separator: "",
                    value: ValueSyntax::Raw,
                },
                after_args: "</function>\n",
            }),
        }
    }

    fn thinking(&self) -> Option<ThinkingSyntax> {
        Some(QWEN_THINKING)
    }

    fn detect(&self, template: &str) -> bool {
        template.contains("<tool_call>") && template.contains("<function=")
    }
}

/// `<start_function_call>call:f{k:<escape>v<escape>}<end_function_call>`
#[derive(Debug)]
pub struct FunctionGemma;

impl OutputFormat for FunctionGemma {
    fn tool_calls(&self) -> ToolCallSyntax {
        ToolCallSyntax {
            before_begin: "",
            begin: "<start_function_call>",
            end: Some("<end_function_call>"),
            after_end: "",
            list: None,
            call: CallSyntax::Parts(CallParts {
                before_name: "call:",
                after_name: "{",
                args: ArgsSyntax::KeyValue {
                    before_key: "",
                    after_key: ":",
                    after_value: "",
                    separator: ", ",
                    value: ValueSyntax::Delimited("<escape>"),
                },
                after_args: "}",
            }),
        }
    }
}

/// `<|tool_call>call:f{k:<|"|>v<|"|>}<tool_call|>`
#[derive(Debug)]
pub struct Gemma4;

impl OutputFormat for Gemma4 {
    fn tool_calls(&self) -> ToolCallSyntax {
        ToolCallSyntax {
            before_begin: "",
            begin: "<|tool_call>",
            end: Some("<tool_call|>"),
            after_end: "",
            list: None,
            call: CallSyntax::Parts(CallParts {
                before_name: "call:",
                after_name: "{",
                args: ArgsSyntax::KeyValue {
                    before_key: "",
                    after_key: ":",
                    after_value: "",
                    separator: ",",
                    value: ValueSyntax::JsonLike { quote: "<|\"|>" },
                },
                after_args: "}",
            }),
        }
    }

    fn thinking(&self) -> Option<ThinkingSyntax> {
        Some(ThinkingSyntax {
            begin: "<|channel>",
            after_begin: "thought\n",
            before_end: "\n",
            end: "<channel|>",
            after_end: "",
        })
    }
}

/// `[TOOL_CALLS]f[ARGS]{"k": "v"}`
#[derive(Debug)]
pub struct Ministral3;

impl OutputFormat for Ministral3 {
    fn tool_calls(&self) -> ToolCallSyntax {
        ToolCallSyntax {
            before_begin: "",
            begin: "[TOOL_CALLS]",
            end: None,
            after_end: "",
            list: None,
            call: CallSyntax::Parts(CallParts {
                before_name: "",
                after_name: "[ARGS]",
                args: ArgsSyntax::Json,
                after_args: "",
            }),
        }
    }

    fn thinking(&self) -> Option<ThinkingSyntax> {
        Some(ThinkingSyntax {
            begin: "[THINK]",
            after_begin: "",
            before_end: "",
            end: "[/THINK]",
            after_end: "",
        })
    }
}

/// `<|tool_call_start|>[f(k="v"), g()]<|tool_call_end|>`
#[derive(Debug)]
pub struct Lfm2;

impl OutputFormat for Lfm2 {
    fn tool_calls(&self) -> ToolCallSyntax {
        ToolCallSyntax {
            before_begin: "",
            begin: "<|tool_call_start|>",
            end: Some("<|tool_call_end|>"),
            after_end: "",
            list: Some(ListSyntax {
                open: "[",
                separator: ", ",
                close: "]",
            }),
            call: CallSyntax::Parts(CallParts {
                before_name: "",
                after_name: "(",
                args: ArgsSyntax::KeyValue {
                    before_key: "",
                    after_key: "=",
                    after_value: "",
                    separator: ", ",
                    value: ValueSyntax::Json,
                },
                after_args: ")",
            }),
        }
    }

    fn thinking(&self) -> Option<ThinkingSyntax> {
        Some(ThinkingSyntax {
            begin: "<think>",
            after_begin: "",
            before_end: "",
            end: "</think>",
            after_end: "",
        })
    }

    fn detect(&self, template: &str) -> bool {
        [
            "<|tool_call_start|>",
            "<|tool_list_start|>",
            "<|tool_response_start|>",
        ]
        .iter()
        .any(|marker| template.contains(marker))
    }
}
