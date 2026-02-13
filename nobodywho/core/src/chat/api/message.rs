use crate::tool_calling::ToolCall;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum Message {
    Message {
        role: Role,
        content: String,
    },
    // it's kind of weird to have the content field in here
    // but according to the qwen3 docs, it should be an empty field on tool call messages
    // https://github.com/QwenLM/Qwen3/blob/e5a1d326/docs/source/framework/function_call.md
    // this also causes a crash when rendering qwen3 chat template, because it tries to get the
    // length of the content field, which is otherwise undefined
    ToolCalls {
        role: Role,
        content: String,
        tool_calls: Vec<ToolCall>,
    },
    ToolResp {
        role: Role,
        name: String,
        content: String,
    },
}

impl Message {
    pub fn role(&self) -> &Role {
        match self {
            Message::Message { role, .. }
            | Message::ToolCalls { role, .. }
            | Message::ToolResp { role, .. } => role,
        }
    }

    pub fn content(&self) -> &str {
        match self {
            Message::Message { content, .. }
            | Message::ToolCalls { content, .. }
            | Message::ToolResp { content, .. } => content,
        }
    }
}
