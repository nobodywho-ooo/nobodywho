use std::sync::LazyLock;

use crate::errors::FromModelError;
use llama_cpp_2::token::LlamaToken;
use minijinja::{context, Environment};
use serde::{self, Deserialize, Serialize};
use serde_json;
use tracing::{trace, warn};

static MINIJINJA_ENV: LazyLock<Environment> = LazyLock::new(|| {
    let mut env = Environment::new();
    env.add_function(
        "raise_exception",
        |msg: String| -> Result<(), minijinja::Error> {
            Err(minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                msg,
            ))
        },
    );
    env.add_function("strftime_now", strftime_now);

    // add a bunch of python-isms, like str.split() or dict.get()
    // was introduced in #106 to fix the deepseek chat template
    env.set_unknown_method_callback(minijinja_contrib::pycompat::unknown_method_callback);
    env
});

fn strftime_now(format_str: &str) -> String {
    chrono::Local::now().format(format_str).to_string()
}

// Chat template typing
// these types generally follow the shape described in the `transformers` docs here:
// https://github.com/huggingface/transformers/blob/b11b28cc4e859558318690a5b41ac3a22644acd5/docs/source/en/chat_templating_writing.md

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
    // length of the content field, which is otherwise undefiend
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
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolType {
    Function,
}

#[derive(Serialize)]
pub struct Function {
    pub name: String,
    pub description: String,
    // TODO: this must be a valid json schema. can we encode that in the type?
    pub parameters: serde_json::Value,
}

#[derive(Serialize)]
pub struct Tool {
    pub r#type: ToolType,
    pub function: Function,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value, // Flexible structure for arbitrary arguments
}

pub struct ChatState {
    messages: Vec<Message>,
    chat_template: String,
    tokens_in_context: Vec<LlamaToken>,
    allow_thinking: bool,
    eos_token: String,
    bos_token: String,
    tools: Vec<Tool>,
}

/// given a chat history where the first two messages are from system and user
/// return a history where the first message is from user, and contains the system prompt as well.
/// (this is what llama.cpp does for the gemma template too)
fn concat_system_and_first_user_messages(
    messages: &[Message],
) -> Result<Vec<Message>, minijinja::Error> {
    warn!("System role not supported by this chat template. Concatenating first user message and system prompt.");
    match messages {
        [Message::Message {
            role: Role::System,
            content: first_content,
        }, Message::Message {
            role: Role::User,
            content: second_content,
        }, rest @ ..] => {
            let new_first_message = Message::Message {
                role: Role::User,
                content: format!("{}\n\n{}", first_content, second_content),
            };
            let new_messages = vec![new_first_message]
                .into_iter()
                .chain(rest.iter().cloned())
                .collect();
            Ok(new_messages)
        }
        _ => {
            // HACK: this should probably be a custom ChatStateError, and nont a minijinja error
            //       but this was quick and easy rn, and we "abuse" the minijinja errors for
            //       `raise_exception` anyway...
            Err(minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                "Cannot replace system prompt unless the first two messages are from system and user roles."
            ))
        }
    }
}

impl ChatState {
    pub fn new(
        chat_template: String,
        bos_token: String,
        eos_token: String,
        tools: Vec<Tool>,
    ) -> Self {
        Self {
            messages: Vec::new(),
            chat_template,
            tokens_in_context: Vec::new(),
            allow_thinking: true,
            eos_token,
            bos_token,
            tools,
        }
    }

    pub fn from_model_and_tools(
        model: &llama_cpp_2::model::LlamaModel,
        tools: Vec<Tool>,
    ) -> Result<Self, FromModelError> {
        let template = Self::select_template(model, !tools.is_empty())?;

        let tokenize = llama_cpp_2::model::Special::Tokenize;
        let bos = model.token_to_str(model.token_bos(), tokenize)?;
        let eos = model.token_to_str(model.token_eos(), tokenize)?;
        Ok(Self::new(template, bos, eos, tools))
    }

    fn select_template(
        model: &llama_cpp_2::model::LlamaModel,
        with_tools: bool,
    ) -> Result<String, FromModelError> {
        let default_template = model.chat_template(None)?.to_string()?;
        let tool_template = model.chat_template(Some("tool_use"));

        let template = if !with_tools {
            // no tools. use default template.
            default_template
        } else if let Ok(tool_template) = tool_template {
            // tools provided, and we have a tool template, use that.
            debug_assert!(tool_template.to_string()?.contains("tools"));
            tool_template.to_string()?
        } else if default_template.contains("tools") {
            // tools provided, but no tool template, but the default template seems to mention tools
            default_template
        } else {
            // tools provided, but we have no tool-capable template
            return Err(FromModelError::NoToolTemplate);
        };
        trace!(template);

        Ok(template)
    }

    pub fn from_model(model: &llama_cpp_2::model::LlamaModel) -> Result<Self, FromModelError> {
        ChatState::from_model_and_tools(model, vec![])
    }

    pub fn reset(&mut self) {
        self.messages = Vec::new();
        self.tokens_in_context = Vec::new();
    }

    pub fn set_allow_thinking(&mut self, allow_thinking: bool) {
        self.allow_thinking = allow_thinking;
    }

    pub fn get_allow_thinking(&self) -> bool {
        self.allow_thinking
    }

    pub fn get_messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn set_messages(&mut self, messages: Vec<Message>) {
        self.reset();
        self.messages = messages;
    }

    pub fn get_tokens_in_context(&self) -> &[LlamaToken] {
        &self.tokens_in_context
    }

    pub fn set_tokens_in_context(&mut self, tokens: Vec<LlamaToken>) {
        self.tokens_in_context = tokens;
    }

    pub fn add_system_message(&mut self, content: String) {
        self.add_message(Role::System, content)
    }

    pub fn add_assistant_message(&mut self, content: String) {
        self.add_message(Role::Assistant, content)
    }

    pub fn add_user_message(&mut self, content: String) {
        self.add_message(Role::User, content)
    }

    fn add_message(&mut self, role: Role, content: String) {
        self.messages.push(Message::Message { role, content });
    }

    pub fn add_tool_calls(&mut self, tool_calls: Vec<ToolCall>) {
        self.messages.push(Message::ToolCalls {
            role: Role::Assistant,
            content: "".into(),
            tool_calls,
        });
    }

    pub fn add_tool_resp(&mut self, name: String, content: String) {
        self.messages.push(Message::ToolResp {
            role: Role::Tool,
            name,
            content,
        });
    }

    /// Prefer using `render_string` which handles also error cases well
    pub fn naive_render_message_vec(
        &self,
        messages: &[Message],
    ) -> Result<String, minijinja::Error> {
        let tmpl = MINIJINJA_ENV.template_from_str(&self.chat_template)?;
        let add_generation_prompt = self.messages.last().is_some_and(|msg| {
            matches!(
                msg,
                Message::Message {
                    role: Role::User,
                    ..
                } | Message::ToolResp { .. }
            )
        });

        let ctx = context! {
            messages => messages,
            add_generation_prompt => add_generation_prompt,
            // we call it allow thinking, because not every model has thinking mode,
            // and 'enable' could then cause confusion
            enable_thinking => self.allow_thinking,
            eos_token => self.eos_token,
            bos_token => self.bos_token,
            tools => self.tools,
        };

        tmpl.render(ctx)
    }

    pub fn render_string(&mut self) -> Result<String, minijinja::Error> {
        let rendered_template = self.naive_render_message_vec(&self.messages);
        let result = match rendered_template {
            Ok(rendered) => Ok(rendered),
            Err(err) => match err.kind() {
                minijinja::ErrorKind::InvalidOperation => {
                    if err.to_string().contains("System role not supported") {
                        // this is the error message we get when rendering the gemma2 template
                        // concat the first two messages and try again
                        self.messages = concat_system_and_first_user_messages(&self.messages)?;
                        self.render_string()
                    } else if err.to_string().contains(
                        "Conversation roles must alternate user/assistant/user/assistant/...",
                    ) {
                        // this is the error we get when rendering the mistral 7b v0.3 template,
                        // which, like gemma2, does not support the system role
                        // concat the first two messages and try again
                        self.messages = concat_system_and_first_user_messages(&self.messages)?;
                        self.render_string()
                    } else {
                        Err(err)
                    }
                }
                _ => Err(err),
            },
        };

        let text = result?;
        trace!(text);

        Ok(text)
    }

    pub fn find_prefix_index_and_difference_with_tokens_in_context(
        &self,
        tokens: &[LlamaToken],
    ) -> (u32, Vec<LlamaToken>) {
        if self.tokens_in_context.is_empty() {
            return (0, tokens.to_owned());
        }

        let longest_common_prefix_index = self
            .tokens_in_context
            .iter()
            .zip(tokens.iter())
            .position(|(a, b)| a != b);

        let (index, difference): (u32, Vec<LlamaToken>) = match longest_common_prefix_index {
            Some(i) => (i as u32, tokens[i..].to_vec()),
            None => (
                self.tokens_in_context.len() as u32,
                tokens[(self.tokens_in_context.len())..].to_vec(),
            ),
        };

        (index, difference)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strftime_now() {
        // huggingface chat template docs say that `strftime_now(format_str)` should be equivalent to `datetime.now().strftime(format_str)`
        // https://huggingface.co/docs/transformers/main/chat_templating#callable-functions

        let result = strftime_now("%Y-%m-%d");
        assert!(
            result.len() == 10,
            "Expected format YYYY-MM-DD to be 10 chars"
        );

        let result = strftime_now("%H:%M:%S");
        assert!(result.len() == 8, "Expected format HH:MM:SS to be 8 chars");
    }

    #[test]
    fn test_render_string_llama3_template() {
        // Llama 3.1 template from the existing test
        let template = "{% set loop_messages = messages %}{% for message in loop_messages %}{% set content = '<|start_header_id|>' + message['role'] + '<|end_header_id|>\n\n'+ message['content'] | trim + '<|eot_id|>' %}{% if loop.index0 == 0 %}{% set content = bos_token + content %}{% endif %}{{ content }}{% endfor %}{{ '<|start_header_id|>assistant<|end_header_id|>\n\n' }}";
        let mut chatstate = ChatState::new(
            template.into(),
            "<|begin_of_text|>".into(),
            "<|end_of_text|>".into(),
            vec![],
        );

        // Test 1: Single user message
        chatstate.add_user_message("Hello, world!".into());
        let rendered = chatstate.render_string().unwrap();

        let expected = "<|begin_of_text|><|start_header_id|>user<|end_header_id|>\n\nHello, world!<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n";
        assert_eq!(rendered, expected);

        // Test 2: Add assistant response
        chatstate.add_assistant_message("Hi there! How can I help?".into());
        let rendered2 = chatstate.render_string().unwrap();

        let expected2 = "<|begin_of_text|><|start_header_id|>user<|end_header_id|>\n\nHello, world!<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\nHi there! How can I help?<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n";
        assert_eq!(rendered2, expected2);

        // Test 3: Multi-turn conversation
        chatstate.add_user_message("What's the weather like?".into());
        chatstate.add_assistant_message("I don't have access to weather data.".into());
        let rendered3 = chatstate.render_string().unwrap();

        assert!(rendered3.starts_with(
            "<|begin_of_text|><|start_header_id|>user<|end_header_id|>\n\nHello, world!<|eot_id|>"
        ));
        assert!(rendered3.contains(
            "<|start_header_id|>user<|end_header_id|>\n\nWhat's the weather like?<|eot_id|>"
        ));
        assert!(rendered3.contains("<|start_header_id|>assistant<|end_header_id|>\n\nI don't have access to weather data.<|eot_id|>"));
        assert!(rendered3.ends_with("<|start_header_id|>assistant<|end_header_id|>\n\n"));

        // Test 4: System message (if added first)
        let mut chatstate_with_system = ChatState::new(
            template.into(),
            "<|begin_of_text|>".into(),
            "<|end_of_text|>".into(),
            vec![],
        );
        chatstate_with_system.add_system_message("You are a helpful assistant.".into());
        chatstate_with_system.add_user_message("Hi".into());
        let rendered4 = chatstate_with_system.render_string().unwrap();

        assert!(rendered4.starts_with("<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\nYou are a helpful assistant.<|eot_id|>"));
        assert!(rendered4.contains("<|start_header_id|>user<|end_header_id|>\n\nHi<|eot_id|>"));
        assert!(rendered4.ends_with("<|start_header_id|>assistant<|end_header_id|>\n\n"));
    }

    #[test]
    fn test_render_string_deepseek_template() {
        // DeepSeek template from the existing test
        let template = "{% if not add_generation_prompt is defined %}{% set add_generation_prompt = false %}{% endif %}{% set ns = namespace(is_first=false, is_tool=false, is_output_first=true, system_prompt='') %}{%- for message in messages %}{%- if message['role'] == 'system' %}{% set ns.system_prompt = message['content'] %}{%- endif %}{%- endfor %}{{bos_token}}{{ns.system_prompt}}{%- for message in messages %}{%- if message['role'] == 'user' %}{%- set ns.is_tool = false -%}{{'<｜User｜>' + message['content']}}{%- endif %}{%- if message['role'] == 'assistant' and message['content'] is none %}{%- set ns.is_tool = false -%}{%- for tool in message['tool_calls']%}{%- if not ns.is_first %}{{'<｜Assistant｜><｜tool▁calls▁begin｜><｜tool▁call▁begin｜>' + tool['type'] + '<｜tool▁sep｜>' + tool['function']['name'] + '\\n' + '```json' + '\\n' + tool['function']['arguments'] + '\\n' + '```' + '<｜tool▁call▁end｜>'}}{%- set ns.is_first = true -%}{%- else %}{{'\\n' + '<｜tool▁call▁begin｜>' + tool['type'] + '<｜tool▁sep｜>' + tool['function']['name'] + '\\n' + '```json' + '\\n' + tool['function']['arguments'] + '\\n' + '```' + '<｜tool▁call▁end｜>'}}{{'<｜tool▁calls▁end｜><｜end▁of▁sentence｜>'}}{%- endif %}{%- endfor %}{%- endif %}{%- if message['role'] == 'assistant' and message['content'] is not none %}{%- if ns.is_tool %}{{'<｜tool▁outputs▁end｜>' + message['content'] + '<｜end▁of▁sentence｜>'}}{%- set ns.is_tool = false -%}{%- else %}{% set content = message['content'] %}{% if '</think>' in content %}{% set content = content.split('</think>')[-1] %}{% endif %}{{'<｜Assistant｜>' + content + '<｜end▁of▁sentence｜>'}}{%- endif %}{%- endif %}{%- if message['role'] == 'tool' %}{%- set ns.is_tool = true -%}{%- if ns.is_output_first %}{{'<｜tool▁outputs▁begin｜><｜tool▁output▁begin｜>' + message['content'] + '<｜tool▁output▁end｜>'}}{%- set ns.is_output_first = false %}{%- else %}{{'\\n<｜tool▁output▁begin｜>' + message['content'] + '<｜tool▁output▁end｜>'}}{%- endif %}{%- endif %}{%- endfor -%}{% if ns.is_tool %}{{'<｜tool▁outputs▁end｜>'}}{% endif %}{% if add_generation_prompt and not ns.is_tool %}{{'<｜Assistant｜>'}}{% endif %}";
        let mut chatstate =
            ChatState::new(template.into(), "<|bos|>".into(), "<|eos|>".into(), vec![]);

        // Test 1: Single user message
        chatstate.add_user_message("Hello, world!".into());
        let rendered = chatstate.render_string().unwrap();

        // ChatState sets add_generation_prompt to true for user messages, so <｜Assistant｜> is added
        let expected = "<|bos|><｜User｜>Hello, world!<｜Assistant｜>";
        assert_eq!(rendered, expected);

        // Test 2: Add assistant response
        chatstate.add_assistant_message("Hi there! How can I help?".into());
        let rendered2 = chatstate.render_string().unwrap();

        let expected2 = "<|bos|><｜User｜>Hello, world!<｜Assistant｜>Hi there! How can I help?<｜end▁of▁sentence｜>";
        assert_eq!(rendered2, expected2);

        // Test 3: Assistant message with thinking block
        chatstate.add_assistant_message(
            "<think>The user is asking for help</think>I'd be happy to assist you!".into(),
        );
        let rendered3 = chatstate.render_string().unwrap();

        // The thinking block should be stripped out, only the content after </think> should remain
        assert!(
            rendered3.contains("<｜Assistant｜>I'd be happy to assist you!<｜end▁of▁sentence｜>")
        );
        assert!(!rendered3.contains("<think>"));
        assert!(!rendered3.contains("</think>"));

        // Test 4: System message
        let mut chatstate_with_system =
            ChatState::new(template.into(), "<|bos|>".into(), "<|eos|>".into(), vec![]);
        chatstate_with_system.add_system_message("You are a helpful assistant.".into());
        chatstate_with_system.add_user_message("Hi".into());
        let rendered4 = chatstate_with_system.render_string().unwrap();

        let expected4 = "<|bos|>You are a helpful assistant.<｜User｜>Hi<｜Assistant｜>";
        assert_eq!(rendered4, expected4);

        // Test 5: Multi-turn conversation
        let mut chatstate5 =
            ChatState::new(template.into(), "<|bos|>".into(), "<|eos|>".into(), vec![]);
        chatstate5.add_user_message("What's 2+2?".into());
        chatstate5.add_assistant_message("4".into());
        chatstate5.add_user_message("Thanks!".into());
        let rendered5 = chatstate5.render_string().unwrap();

        let expected5 =
            "<|bos|><｜User｜>What's 2+2?<｜Assistant｜>4<｜end▁of▁sentence｜><｜User｜>Thanks!<｜Assistant｜>";
        assert_eq!(rendered5, expected5);

        // Test 6: Empty messages (no generation prompt by default)
        let mut chatstate6 =
            ChatState::new(template.into(), "<|bos|>".into(), "<|eos|>".into(), vec![]);
        let rendered6 = chatstate6.render_string().unwrap();

        let expected6 = "<|bos|>";
        assert_eq!(rendered6, expected6);
    }

    #[test]
    fn test_render_string_qwen3_template() {
        // Qwen3 template from the existing test
        let template = "{%- if tools %}\n    {{- '<|im_start|>system\\n' }}\n    {%- if messages[0].role == 'system' %}\n        {{- messages[0].content + '\\n\\n' }}\n    {%- endif %}\n    {{- \"# Tools\\n\\nYou may call one or more functions to assist with the user query.\\n\\nYou are provided with function signatures within <tools></tools> XML tags:\\n<tools>\" }}\n    {%- for tool in tools %}\n        {{- \"\\n\" }}\n        {{- tool | tojson }}\n    {%- endfor %}\n    {{- \"\\n</tools>\\n\\nFor each function call, return a json object with function name and arguments within <tool_call></tool_call> XML tags:\\n<tool_call>\\n{\\\"name\\\": <function-name>, \\\"arguments\\\": <args-json-object>}\\n</tool_call><|im_end|>\\n\" }}\n{%- else %}\n    {%- if messages[0].role == 'system' %}\n        {{- '<|im_start|>system\\n' + messages[0].content + '<|im_end|>\\n' }}\n    {%- endif %}\n{%- endif %}\n{%- set ns = namespace(multi_step_tool=true, last_query_index=messages|length - 1) %}\n{%- for message in messages[::-1] %}\n    {%- set index = (messages|length - 1) - loop.index0 %}\n    {%- if ns.multi_step_tool and message.role == \"user\" and not(message.content.startswith('<tool_response>') and message.content.endswith('</tool_response>')) %}\n        {%- set ns.multi_step_tool = false %}\n        {%- set ns.last_query_index = index %}\n    {%- endif %}\n{%- endfor %}\n{%- for message in messages %}\n    {%- if (message.role == \"user\") or (message.role == \"system\" and not loop.first) %}\n        {{- '<|im_start|>' + message.role + '\\n' + message.content + '<|im_end|>' + '\\n' }}\n    {%- elif message.role == \"assistant\" %}\n        {%- set content = message.content %}\n        {%- set reasoning_content = '' %}\n        {%- if message.reasoning_content is defined and message.reasoning_content is not none %}\n            {%- set reasoning_content = message.reasoning_content %}\n        {%- else %}\n            {%- if '</think>' in message.content %}\n                {%- set content = message.content.split('</think>')[-1].lstrip('\\n') %}\n                {%- set reasoning_content = message.content.split('</think>')[0].rstrip('\\n').split('<think>')[-1].lstrip('\\n') %}\n            {%- endif %}\n        {%- endif %}\n        {%- if loop.index0 > ns.last_query_index %}\n            {%- if loop.last or (not loop.last and reasoning_content) %}\n                {{- '<|im_start|>' + message.role + '\\n<think>\\n' + reasoning_content.strip('\\n') + '\\n</think>\\n\\n' + content.lstrip('\\n') }}\n            {%- else %}\n                {{- '<|im_start|>' + message.role + '\\n' + content }}\n            {%- endif %}\n        {%- else %}\n            {{- '<|im_start|>' + message.role + '\\n' + content }}\n        {%- endif %}\n        {%- if message.tool_calls %}\n            {%- for tool_call in message.tool_calls %}\n                {%- if (loop.first and content) or (not loop.first) %}\n                    {{- '\\n' }}\n                {%- endif %}\n                {%- if tool_call.function %}\n                    {%- set tool_call = tool_call.function %}\n                {%- endif %}\n                {{- '<tool_call>\\n{\"name\": \"' }}\n                {{- tool_call.name }}\n                {{- '\", \"arguments\": ' }}\n                {%- if tool_call.arguments is string %}\n                    {{- tool_call.arguments }}\n                {%- else %}\n                    {{- tool_call.arguments | tojson }}\n                {%- endif %}\n                {{- '}\\n</tool_call>' }}\n            {%- endfor %}\n        {%- endif %}\n        {{- '<|im_end|>\\n' }}\n    {%- elif message.role == \"tool\" %}\n        {%- if loop.first or (messages[loop.index0 - 1].role != \"tool\") %}\n            {{- '<|im_start|>user' }}\n        {%- endif %}\n        {{- '\\n<tool_response>\\n' }}\n        {{- message.content }}\n        {{- '\\n</tool_response>' }}\n        {%- if loop.last or (messages[loop.index0 + 1].role != \"tool\") %}\n            {{- '<|im_end|>\\n' }}\n        {%- endif %}\n    {%- endif %}\n{%- endfor %}\n{%- if add_generation_prompt %}\n    {{- '<|im_start|>assistant\\n' }}\n    {%- if enable_thinking is defined and enable_thinking is false %}\n        {{- '<think>\\n\\n</think>\\n\\n' }}\n    {%- endif %}\n{%- endif %}";
        let mut chatstate = ChatState::new(template.into(), "".into(), "".into(), vec![]);

        // Test 1: Single user message
        chatstate.add_user_message("Hi, robot!".into());
        let rendered = chatstate.render_string().unwrap();

        let expected = "<|im_start|>user\nHi, robot!<|im_end|>\n<|im_start|>assistant\n";
        assert_eq!(rendered, expected);

        // Test 2: Add assistant response with thinking
        chatstate.add_assistant_message("<think>\nHm... That's a tough cookie. I think the answer is probably 42.\nCould it be something else?\nNah... It's 42!\n</think>\nThe answer is 42!".into());
        let rendered2 = chatstate.render_string().unwrap();

        // The thinking block should be included in the output for Qwen3
        let expected2 = "<|im_start|>user\nHi, robot!<|im_end|>\n<|im_start|>assistant\n<think>\nHm... That's a tough cookie. I think the answer is probably 42.\nCould it be something else?\nNah... It's 42!\n</think>\n\nThe answer is 42!<|im_end|>\n";
        assert_eq!(rendered2, expected2);

        // Test 3: System message
        let mut chatstate_with_system =
            ChatState::new(template.into(), "".into(), "".into(), vec![]);
        chatstate_with_system.add_system_message("You are a helpful assistant.".into());
        chatstate_with_system.add_user_message("Hello".into());
        let rendered3 = chatstate_with_system.render_string().unwrap();

        let expected3 = "<|im_start|>system\nYou are a helpful assistant.<|im_end|>\n<|im_start|>user\nHello<|im_end|>\n<|im_start|>assistant\n";
        assert_eq!(rendered3, expected3);

        // Test 4: Multi-turn conversation
        let mut chatstate4 = ChatState::new(template.into(), "".into(), "".into(), vec![]);
        chatstate4.add_user_message("What's 2+2?".into());
        chatstate4.add_assistant_message("4".into());
        chatstate4.add_user_message("Thanks!".into());
        let rendered4 = chatstate4.render_string().unwrap();

        let expected4 = "<|im_start|>user\nWhat's 2+2?<|im_end|>\n<|im_start|>assistant\n4<|im_end|>\n<|im_start|>user\nThanks!<|im_end|>\n<|im_start|>assistant\n";
        assert_eq!(rendered4, expected4);

        // Test 5: Assistant message without thinking
        let mut chatstate5 = ChatState::new(template.into(), "".into(), "".into(), vec![]);
        chatstate5.add_user_message("Hello".into());
        chatstate5.add_assistant_message("Hi there!".into());
        let rendered5 = chatstate5.render_string().unwrap();

        // The template now includes empty thinking blocks for assistant messages
        let expected5 = "<|im_start|>user\nHello<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\nHi there!<|im_end|>\n";
        assert_eq!(rendered5, expected5);
    }
}
