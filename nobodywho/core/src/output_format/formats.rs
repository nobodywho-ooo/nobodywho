use super::{
    ArgsSyntax, CallParts, CallSyntax, ListSyntax, OutputFormat, ThinkingSyntax, ToolCallSyntax,
    ValueSyntax,
};

const THINK_TAGS: ThinkingSyntax = ThinkingSyntax {
    begin: "<think>",
    label: "",
    end: "</think>",
};

/// `<tool_call>\n{"name": "f", "arguments": {"k": "v"}}\n</tool_call>`
#[derive(Debug)]
pub struct Qwen3;

impl OutputFormat for Qwen3 {
    fn tool_calls(&self) -> ToolCallSyntax {
        ToolCallSyntax {
            begin: "<tool_call>",
            end: Some("</tool_call>"),
            list: None,
            call: CallSyntax::JsonObject {
                name_key: "name",
                arguments_key: "arguments",
            },
        }
    }

    fn thinking(&self) -> Option<ThinkingSyntax> {
        Some(THINK_TAGS)
    }
}

/// `<tool_call>\n<function=f>\n<parameter=k>\nv\n</parameter>\n</function>\n</tool_call>`
#[derive(Debug)]
pub struct Qwen35;

impl OutputFormat for Qwen35 {
    fn tool_calls(&self) -> ToolCallSyntax {
        ToolCallSyntax {
            begin: "<tool_call>",
            end: Some("</tool_call>"),
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
        Some(THINK_TAGS)
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
            begin: "<start_function_call>",
            end: Some("<end_function_call>"),
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
            begin: "<|tool_call>",
            end: Some("<tool_call|>"),
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
            label: "thought\n",
            end: "<channel|>",
        })
    }
}

/// `[TOOL_CALLS]f[ARGS]{"k": "v"}`
#[derive(Debug)]
pub struct Ministral3;

impl OutputFormat for Ministral3 {
    fn tool_calls(&self) -> ToolCallSyntax {
        ToolCallSyntax {
            begin: "[TOOL_CALLS]",
            end: None,
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
            label: "",
            end: "[/THINK]",
        })
    }
}

/// `<|tool_call_start|>[f(k="v"), g()]<|tool_call_end|>`
#[derive(Debug)]
pub struct Lfm2;

impl OutputFormat for Lfm2 {
    fn tool_calls(&self) -> ToolCallSyntax {
        ToolCallSyntax {
            begin: "<|tool_call_start|>",
            end: Some("<|tool_call_end|>"),
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
        Some(THINK_TAGS)
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
