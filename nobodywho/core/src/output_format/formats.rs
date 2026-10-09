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
pub fn qwen3() -> OutputFormat {
    const TOOL_CALLS: ToolCallSyntax = ToolCallSyntax {
        before_begin: "\n",
        begin: "<tool_call>",
        end: Some("</tool_call>"),
        after_end: "",
        list: None,
        several_blocks_allowed: true,
        call: CallSyntax::JsonObject {
            name_key: "name",
            arguments_key: "arguments",
        },
    };
    OutputFormat {
        name: "Qwen3",
        tool_calls: TOOL_CALLS,
        thinking: Some(QWEN_THINKING),
    }
}

/// `<tool_call>\n<function=f>\n<parameter=k>\nv\n</parameter>\n</function>\n</tool_call>`
pub fn qwen35() -> OutputFormat {
    // `\n\n` after text, but only `\n` between calls.
    const TOOL_CALLS: ToolCallSyntax = ToolCallSyntax {
        before_begin: "\n\n",
        begin: "<tool_call>",
        end: Some("</tool_call>"),
        after_end: "\n",
        list: None,
        several_blocks_allowed: true,
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
    };
    OutputFormat {
        name: "Qwen35",
        tool_calls: TOOL_CALLS,
        thinking: Some(QWEN_THINKING),
    }
}

/// `<start_function_call>call:f{k:<escape>v<escape>}<end_function_call>`
pub fn function_gemma() -> OutputFormat {
    const TOOL_CALLS: ToolCallSyntax = ToolCallSyntax {
        before_begin: "",
        begin: "<start_function_call>",
        end: Some("<end_function_call>"),
        after_end: "",
        list: None,
        several_blocks_allowed: false,
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
    };
    OutputFormat {
        name: "FunctionGemma",
        tool_calls: TOOL_CALLS,
        thinking: None,
    }
}

/// `<|tool_call>call:f{k:<|"|>v<|"|>}<tool_call|>`
pub fn gemma4() -> OutputFormat {
    const TOOL_CALLS: ToolCallSyntax = ToolCallSyntax {
        before_begin: "",
        begin: "<|tool_call>",
        end: Some("<tool_call|>"),
        after_end: "",
        list: None,
        several_blocks_allowed: true,
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
    };
    OutputFormat {
        name: "Gemma4",
        tool_calls: TOOL_CALLS,
        thinking: Some(ThinkingSyntax {
            begin: "<|channel>",
            after_begin: "thought\n",
            before_end: "\n",
            end: "<channel|>",
            after_end: "",
        }),
    }
}

/// `[TOOL_CALLS]f[ARGS]{"k": "v"}`
pub fn ministral3() -> OutputFormat {
    const TOOL_CALLS: ToolCallSyntax = ToolCallSyntax {
        before_begin: "",
        begin: "[TOOL_CALLS]",
        end: None,
        after_end: "",
        list: None,
        several_blocks_allowed: true,
        call: CallSyntax::Parts(CallParts {
            before_name: "",
            after_name: "[ARGS]",
            args: ArgsSyntax::Json,
            after_args: "",
        }),
    };
    OutputFormat {
        name: "Ministral3",
        tool_calls: TOOL_CALLS,
        thinking: Some(ThinkingSyntax {
            begin: "[THINK]",
            after_begin: "",
            before_end: "",
            end: "[/THINK]",
            after_end: "",
        }),
    }
}

/// `<|tool_call_start|>[f(k="v"), g()]<|tool_call_end|>`
pub fn lfm2() -> OutputFormat {
    const TOOL_CALLS: ToolCallSyntax = ToolCallSyntax {
        before_begin: "",
        begin: "<|tool_call_start|>",
        end: Some("<|tool_call_end|>"),
        after_end: "",
        list: Some(ListSyntax {
            open: "[",
            separator: ", ",
            close: "]",
        }),
        several_blocks_allowed: false,
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
    };
    OutputFormat {
        name: "Lfm2",
        tool_calls: TOOL_CALLS,
        thinking: Some(ThinkingSyntax {
            begin: "<think>",
            after_begin: "",
            before_end: "",
            end: "</think>",
            after_end: "",
        }),
    }
}
