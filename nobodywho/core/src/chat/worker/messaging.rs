use crate::errors::ChatWorkerError;
use crate::llm::{self, Worker};
use crate::sampler_config::SamplerConfig;
use crate::tool_calling::Tool;
use tracing::info;

use super::super::types::Message;
use super::ChatWorker;

pub(crate) enum ChatMsg {
    Ask {
        text: String,
        output_tx: tokio::sync::mpsc::Sender<llm::WriteOutput>,
    },
    ResetChat {
        system_prompt: String,
        tools: Vec<Tool>,
        output_tx: tokio::sync::mpsc::Sender<()>,
    },
    SetTools {
        tools: Vec<Tool>,
        output_tx: tokio::sync::mpsc::Sender<()>,
    },
    SetSystemPrompt {
        system_prompt: String,
        output_tx: tokio::sync::mpsc::Sender<()>,
    },
    SetThinking {
        allow_thinking: bool,
        output_tx: tokio::sync::mpsc::Sender<()>,
    },
    SetSamplerConfig {
        sampler_config: SamplerConfig,
        output_tx: tokio::sync::mpsc::Sender<()>,
    },
    GetChatHistory {
        output_tx: tokio::sync::mpsc::Sender<Vec<Message>>,
    },
    SetChatHistory {
        messages: Vec<Message>,
        output_tx: tokio::sync::mpsc::Sender<()>,
    },
}

impl std::fmt::Debug for ChatMsg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatMsg::Ask { text, .. } => f.debug_struct("Ask").field("text", text).finish(),
            ChatMsg::ResetChat {
                system_prompt,
                tools,
                ..
            } => f
                .debug_struct("ResetChat")
                .field("system_prompt", system_prompt)
                .field("tools", &format!("[{} tools]", tools.len()))
                .finish(),
            ChatMsg::SetTools { tools, .. } => f
                .debug_struct("SetTools")
                .field("tools", &format!("[{} tools]", tools.len()))
                .finish(),
            ChatMsg::SetSystemPrompt { system_prompt, .. } => f
                .debug_struct("SetSystemPrompt")
                .field("system_prompt", system_prompt)
                .finish(),
            ChatMsg::SetThinking { allow_thinking, .. } => f
                .debug_struct("SetThinking")
                .field("allow_thinking", allow_thinking)
                .finish(),
            ChatMsg::SetSamplerConfig { sampler_config, .. } => f
                .debug_struct("SetSamplerConfig")
                .field("sampler_config", sampler_config)
                .finish(),
            ChatMsg::GetChatHistory { .. } => f.debug_struct("GetChatHistory").finish(),
            ChatMsg::SetChatHistory { messages, .. } => f
                .debug_struct("SetChatHistory")
                .field("messages", &format!("[{} messages]", messages.len()))
                .finish(),
        }
    }
}

pub(crate) fn process_worker_msg(
    worker_state: &mut Worker<'_, ChatWorker>,
    msg: ChatMsg,
) -> Result<(), ChatWorkerError> {
    info!(?msg, "Worker processing:");
    match msg {
        ChatMsg::Ask { text, output_tx } => {
            let callback = move |out| {
                let _ = output_tx.blocking_send(out);
            };
            worker_state.ask(text, callback)?;
        }
        ChatMsg::ResetChat {
            system_prompt,
            tools,
            output_tx,
        } => {
            worker_state.reset_chat(system_prompt, tools)?;
            let _ = output_tx.blocking_send(());
        }
        ChatMsg::SetTools { tools, output_tx } => {
            worker_state.set_tools(tools)?;
            let _ = output_tx.blocking_send(());
        }
        ChatMsg::SetSystemPrompt {
            system_prompt,
            output_tx,
        } => {
            worker_state.set_system_prompt(system_prompt)?;
            let _ = output_tx.blocking_send(());
        }
        ChatMsg::SetThinking {
            allow_thinking,
            output_tx,
        } => {
            worker_state.set_allow_thinking(allow_thinking)?;
            let _ = output_tx.blocking_send(());
        }
        ChatMsg::SetSamplerConfig {
            sampler_config,
            output_tx,
        } => {
            worker_state.set_sampler_config(sampler_config);
            let _ = output_tx.blocking_send(());
        }
        ChatMsg::GetChatHistory { output_tx } => {
            let msgs = worker_state.get_chat_history();
            let _ = output_tx.blocking_send(msgs);
        }
        ChatMsg::SetChatHistory {
            messages,
            output_tx,
        } => {
            worker_state.set_chat_history(messages)?;
            let _ = output_tx.blocking_send(());
        }
    };

    Ok(())
}
