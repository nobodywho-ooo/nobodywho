use std::sync::LazyLock;

use llama_cpp_2::{
    model::{AddBos, LlamaModel},
    token::LlamaToken,
};
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

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

#[derive(Deserialize, Serialize, Clone)]
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
    prefix_cache: Vec<LlamaToken>,
    length: usize,
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

#[derive(Debug, thiserror::Error)]
pub enum FromModelError {
    #[error("Lama.cpp failed fetching chat template from the model file. This is likely because you're using an older GGUF file, which might not include a chat template. For example, this is the case for most LLaMA2-based GGUF files. Try using a more recent GGUF model file. If you want to check if a given model includes a chat template, you can use the gguf-dump script from llama.cpp. Here is a more technical detailed error: {0}")]
    ChatTemplateError(#[from] llama_cpp_2::ChatTemplateError),

    #[error("Could not parse chat template as UTF8: {0}")]
    TemplateUtf8Error(#[from] std::str::Utf8Error),

    #[error("Could not detokenize string: {0}")]
    Detokenize(#[from] llama_cpp_2::TokenToStringError),

    #[error("Tools were provided, but it looks like this model doesn't support tool calling.")]
    NoToolTemplateError,
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("Template failed to render: {0}")]
    MiniJinjaError(#[from] minijinja::Error),

    #[error("Could not tokenize string: {0}")]
    CreateContextError(#[from] llama_cpp_2::StringToTokenError),
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
            prefix_cache: Vec::new(),
            length: 0,
            eos_token,
            bos_token,
            tools,
        }
    }

    pub fn from_model(model: &llama_cpp_2::model::LlamaModel) -> Result<Self, FromModelError> {
        let template = model.chat_template(None)?.to_string()?;
        let tokenize = llama_cpp_2::model::Special::Tokenize;
        let bos = model.token_to_str(model.token_bos(), tokenize)?;
        let eos = model.token_to_str(model.token_eos(), tokenize)?;
        Ok(Self::new(template, bos, eos, vec![]))
    }

    pub fn from_model_and_tools(
        model: &llama_cpp_2::model::LlamaModel,
        tools: Vec<Tool>,
    ) -> Result<Self, FromModelError> {
        let default_template = model.chat_template(None)?.to_string()?;
        let tool_template = model.chat_template(Some("tool_use"));

        let template = if tools.len() == 0 {
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
            return Err(FromModelError::NoToolTemplateError);
        };
        trace!(template);

        let tokenize = llama_cpp_2::model::Special::Tokenize;
        let bos = model.token_to_str(model.token_bos(), tokenize)?;
        let eos = model.token_to_str(model.token_eos(), tokenize)?;
        Ok(Self::new(template, bos, eos, tools))
    }

    pub fn reset(&mut self) {
        self.length = 0;
        self.messages = Vec::new();
    }

    pub fn get_messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn set_messages(&mut self, messages: Vec<Message>) {
        self.reset();
        self.messages = messages;
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

    fn render_string(&mut self) -> Result<String, minijinja::Error> {
        let tmpl = MINIJINJA_ENV.template_from_str(&self.chat_template)?;

        let ctx = context! {
            messages => &self.messages,
            add_generation_prompt => self.messages.last().map_or(false, |msg| match msg {
                Message::Message { role: Role::User, .. } => true,
                Message::ToolResp { .. } => true,
                _ => false,
            }),
            eos_token => self.eos_token,
            bos_token => self.bos_token,
            tools => self.tools,
        };

        let result = match tmpl.render(ctx) {
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

    fn render_tokens(&mut self, ctx_model: &LlamaModel) -> Result<Vec<LlamaToken>, RenderError> {
        let text = self.render_string()?;
        let tokens = ctx_model.str_to_token(&text, AddBos::Never)?;
        trace!(text);

        Ok(tokens)
    }

    pub fn find_token_diff_and_prefix_index(
        &mut self,
        ctx_model: &LlamaModel,
    ) -> Result<(u32, Vec<LlamaToken>), RenderError> {
        let tokens = self.render_tokens(ctx_model)?;

        if self.prefix_cache.len() == 0 {
            self.prefix_cache = tokens.clone();
            return Ok((0, tokens));
        }

        let longest_common_prefix_index = self
            .prefix_cache
            .iter()
            .zip(tokens.iter())
            .position(|(a, b)| a != b);

        let (index, diff): (u32, Vec<LlamaToken>) = match longest_common_prefix_index {
            Some(i) => (i as u32, tokens[i..].iter().cloned().collect()),
            None => (
                self.prefix_cache.len() as u32,
                tokens[(self.prefix_cache.len())..]
                    .iter()
                    .cloned()
                    .collect(),
            ),
        };

        self.prefix_cache = tokens;
        return Ok((index, diff));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llama31_template_prefix_caching() {
        // Load test model for tokenization
        let model = crate::test_utils::load_test_model();

        let template = "{% set loop_messages = messages %}{% for message in loop_messages %}{% set content = '<|start_header_id|>' + message['role'] + '<|end_header_id|>\n\n'+ message['content'] | trim + '<|eot_id|>' %}{% if loop.index0 == 0 %}{% set content = bos_token + content %}{% endif %}{{ content }}{% endfor %}{{ '<|start_header_id|>assistant<|end_header_id|>\n\n' }}";
        let mut chatstate =
            ChatState::new(template.into(), "<|bos|>".into(), "<|eos|>".into(), vec![]);

        // First call - should return all tokens as diff (no cache)
        chatstate.add_user_message("Hello, world!".into());
        let (prefix_index1, diff1) = chatstate.find_token_diff_and_prefix_index(&model).unwrap();

        assert_eq!(prefix_index1, 0, "First call should have no prefix cached");
        assert!(
            diff1.len() > 0,
            "First call should return all tokens as diff"
        );

        // Cache should now be populated with the full token sequence
        let cache_len_after_first = chatstate.prefix_cache.len();
        assert_eq!(
            cache_len_after_first,
            diff1.len(),
            "Cache should contain all tokens from first call"
        );

        // Second call with same message - should return empty diff
        let (prefix_index2, diff2) = chatstate.find_token_diff_and_prefix_index(&model).unwrap();

        assert_eq!(
            prefix_index2, cache_len_after_first as u32,
            "Second call should find full prefix match"
        );
        assert_eq!(
            diff2.len(),
            0,
            "Second call should return empty diff (no new tokens)"
        );

        // Third call with additional message - should return only new tokens as diff
        chatstate.add_assistant_message("Hi there!".into());
        let (prefix_index3, diff3) = chatstate.find_token_diff_and_prefix_index(&model).unwrap();

        assert!(
            prefix_index3 > 0,
            "Third call should find some prefix match"
        );
        assert!(
            diff3.len() > 0,
            "Third call should return new tokens as diff"
        );
        assert!(
            prefix_index3 < chatstate.prefix_cache.len() as u32,
            "Prefix should be less than total cache length"
        );

        // Verify the cache is updated correctly
        let expected_full_tokens = chatstate.render_tokens(&model).unwrap();
        assert_eq!(
            chatstate.prefix_cache, expected_full_tokens,
            "Cache should match full token sequence"
        );
    }

    #[test]
    fn test_prefix_caching_edge_cases() {
        let model = crate::test_utils::load_test_model();
        let template =
            "{% for message in messages %}{{ message.role }}: {{ message.content }}{% endfor %}";
        let mut chatstate = ChatState::new(template.into(), "".into(), "".into(), vec![]);

        // Test 1: Empty messages - should work (empty template produces empty tokens)
        let (prefix_index, diff) = chatstate.find_token_diff_and_prefix_index(&model).unwrap();
        assert_eq!(prefix_index, 0);
        // Empty template with no messages produces empty token sequence
        assert_eq!(diff.len(), 0, "Empty template should produce empty tokens");

        // Test 2: Add message and ensure partial prefix matching
        chatstate.add_user_message("Hello".into());
        let (_prefix_index1, _diff1) = chatstate.find_token_diff_and_prefix_index(&model).unwrap();

        // Test 3: Same message again - should return empty diff
        let (prefix_index2, diff2) = chatstate.find_token_diff_and_prefix_index(&model).unwrap();
        assert_eq!(diff2.len(), 0, "Identical render should produce empty diff");
        assert_eq!(prefix_index2, chatstate.prefix_cache.len() as u32);

        // Test 4: Modify existing message (simulate chat state change)
        chatstate.messages[0] = Message::Message {
            role: Role::User,
            content: "Hello World".into(),
        };
        let (prefix_index3, diff3) = chatstate.find_token_diff_and_prefix_index(&model).unwrap();

        // Should find some common prefix but have differences
        assert!(prefix_index3 > 0, "Should find some common prefix");
        assert!(diff3.len() > 0, "Should have different tokens");

        println!(
            "Test 4: prefix_index={}, diff_len={}",
            prefix_index3,
            diff3.len()
        );

        // Test 5: Completely different messages
        let mut new_chatstate = ChatState::new(template.into(), "".into(), "".into(), vec![]);
        new_chatstate.add_assistant_message("Goodbye".into());
        new_chatstate.prefix_cache = chatstate.prefix_cache.clone(); // Simulate existing cache

        let (prefix_index4, diff4) = new_chatstate
            .find_token_diff_and_prefix_index(&model)
            .unwrap();

        // Depending on tokenization, might have some common prefix from template
        assert!(diff4.len() > 0, "Should have different content");

        println!(
            "Test 5: prefix_index={}, diff_len={}",
            prefix_index4,
            diff4.len()
        );
    }

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
    fn test_deepseek_template_prefix_caching() {
        let model = crate::test_utils::load_test_model();
        let template = "{% if not add_generation_prompt is defined %}{% set add_generation_prompt = false %}{% endif %}{% set ns = namespace(is_first=false, is_tool=false, is_output_first=true, system_prompt='') %}{%- for message in messages %}{%- if message['role'] == 'system' %}{% set ns.system_prompt = message['content'] %}{%- endif %}{%- endfor %}{{bos_token}}{{ns.system_prompt}}{%- for message in messages %}{%- if message['role'] == 'user' %}{%- set ns.is_tool = false -%}{{'<｜User｜>' + message['content']}}{%- endif %}{%- if message['role'] == 'assistant' and message['content'] is none %}{%- set ns.is_tool = false -%}{%- for tool in message['tool_calls']%}{%- if not ns.is_first %}{{'<｜Assistant｜><｜tool▁calls▁begin｜><｜tool▁call▁begin｜>' + tool['type'] + '<｜tool▁sep｜>' + tool['function']['name'] + '\\n' + '```json' + '\\n' + tool['function']['arguments'] + '\\n' + '```' + '<｜tool▁call▁end｜>'}}{%- set ns.is_first = true -%}{%- else %}{{'\\n' + '<｜tool▁call▁begin｜>' + tool['type'] + '<｜tool▁sep｜>' + tool['function']['name'] + '\\n' + '```json' + '\\n' + tool['function']['arguments'] + '\\n' + '```' + '<｜tool▁call▁end｜>'}}{{'<｜tool▁calls▁end｜><｜end▁of▁sentence｜>'}}{%- endif %}{%- endfor %}{%- endif %}{%- if message['role'] == 'assistant' and message['content'] is not none %}{%- if ns.is_tool %}{{'<｜tool▁outputs▁end｜>' + message['content'] + '<｜end▁of▁sentence｜>'}}{%- set ns.is_tool = false -%}{%- else %}{% set content = message['content'] %}{% if '</think>' in content %}{% set content = content.split('</think>')[-1] %}{% endif %}{{'<｜Assistant｜>' + content + '<｜end▁of▁sentence｜>'}}{%- endif %}{%- endif %}{%- if message['role'] == 'tool' %}{%- set ns.is_tool = true -%}{%- if ns.is_output_first %}{{'<｜tool▁outputs▁begin｜><｜tool▁output▁begin｜>' + message['content'] + '<｜tool▁output▁end｜>'}}{%- set ns.is_output_first = false %}{%- else %}{{'\\n<｜tool▁output▁begin｜>' + message['content'] + '<｜tool▁output▁end｜>'}}{%- endif %}{%- endif %}{%- endfor -%}{% if ns.is_tool %}{{'<｜tool▁outputs▁end｜>'}}{% endif %}{% if add_generation_prompt and not ns.is_tool %}{{'<｜Assistant｜>'}}{% endif %}";
        let mut chatstate =
            ChatState::new(template.into(), "<|bos|>".into(), "<|eos|>".into(), vec![]);

        // Test 1: First message - establishes baseline cache
        chatstate.add_user_message("Hello, world!".into());
        let (prefix_index1, diff1) = chatstate.find_token_diff_and_prefix_index(&model).unwrap();

        assert_eq!(prefix_index1, 0, "First call should have no prefix cached");
        assert!(
            diff1.len() > 0,
            "First call should return all tokens as diff"
        );

        let initial_cache_size = chatstate.prefix_cache.len();
        println!("Initial cache size: {}", initial_cache_size);

        // Test 2: Add assistant message with think block - should find common prefix
        chatstate.add_assistant_message("<think>beep boop robot thinky</think>".into());
        let (prefix_index2, diff2) = chatstate.find_token_diff_and_prefix_index(&model).unwrap();

        assert!(
            prefix_index2 > 0,
            "Should find some common prefix from BOS token and user message"
        );
        assert!(
            diff2.len() > 0,
            "Should have new tokens for assistant message"
        );
        assert!(
            prefix_index2 < chatstate.prefix_cache.len() as u32,
            "Prefix should be less than total cache"
        );

        println!(
            "After assistant message: prefix_index={}, diff_len={}, total_cache={}",
            prefix_index2,
            diff2.len(),
            chatstate.prefix_cache.len()
        );

        // Test 3: Same messages again - should return empty diff
        let (prefix_index3, diff3) = chatstate.find_token_diff_and_prefix_index(&model).unwrap();

        assert_eq!(
            diff3.len(),
            0,
            "Identical template render should produce empty diff"
        );
        assert_eq!(
            prefix_index3,
            chatstate.prefix_cache.len() as u32,
            "Should match full cache length"
        );

        // Test 4: Verify cache consistency
        let expected_tokens = chatstate.render_tokens(&model).unwrap();
        assert_eq!(
            chatstate.prefix_cache, expected_tokens,
            "Cache should exactly match rendered tokens"
        );

        // Test 5: Verify template renders correctly (basic sanity check)
        let rendered_text = chatstate.render_string().unwrap();
        assert!(
            rendered_text.contains("<|bos|>"),
            "Should contain BOS token"
        );
        assert!(
            rendered_text.contains("<｜User｜>Hello, world!"),
            "Should contain user message"
        );
        assert!(
            rendered_text.contains("<｜Assistant｜>"),
            "Should contain assistant tag (think block should be stripped)"
        );

        println!("Final rendered text length: {}", rendered_text.len());
    }

    #[test]
    fn test_qwen3_template_prefix_caching() {
        let model = crate::test_utils::load_test_model();
        let template = "{%- if tools %}\n    {{- '<|im_start|>system\\n' }}\n    {%- if messages[0].role == 'system' %}\n        {{- messages[0].content + '\\n\\n' }}\n    {%- endif %}\n    {{- \"# Tools\\n\\nYou may call one or more functions to assist with the user query.\\n\\nYou are provided with function signatures within <tools></tools> XML tags:\\n<tools>\" }}\n    {%- for tool in tools %}\n        {{- \"\\n\" }}\n        {{- tool | tojson }}\n    {%- endfor %}\n    {{- \"\\n</tools>\\n\\nFor each function call, return a json object with function name and arguments within <tool_call></tool_call> XML tags:\\n<tool_call>\\n{\\\"name\\\": <function-name>, \\\"arguments\\\": <args-json-object>}\\n</tool_call><|im_end|>\\n\" }}\n{%- else %}\n    {%- if messages[0].role == 'system' %}\n        {{- '<|im_start|>system\\n' + messages[0].content + '<|im_end|>\\n' }}\n    {%- endif %}\n{%- endif %}\n{%- set ns = namespace(multi_step_tool=true, last_query_index=messages|length - 1) %}\n{%- for message in messages[::-1] %}\n    {%- set index = (messages|length - 1) - loop.index0 %}\n    {%- if ns.multi_step_tool and message.role == \"user\" and not(message.content.startswith('<tool_response>') and message.content.endswith('</tool_response>')) %}\n        {%- set ns.multi_step_tool = false %}\n        {%- set ns.last_query_index = index %}\n    {%- endif %}\n{%- endfor %}\n{%- for message in messages %}\n    {%- if (message.role == \"user\") or (message.role == \"system\" and not loop.first) %}\n        {{- '<|im_start|>' + message.role + '\\n' + message.content + '<|im_end|>' + '\\n' }}\n    {%- elif message.role == \"assistant\" %}\n        {%- set content = message.content %}\n        {%- set reasoning_content = '' %}\n        {%- if message.reasoning_content is defined and message.reasoning_content is not none %}\n            {%- set reasoning_content = message.reasoning_content %}\n        {%- else %}\n            {%- if '</think>' in message.content %}\n                {%- set content = message.content.split('</think>')[-1].lstrip('\\n') %}\n                {%- set reasoning_content = message.content.split('</think>')[0].rstrip('\\n').split('<think>')[-1].lstrip('\\n') %}\n            {%- endif %}\n        {%- endif %}\n        {%- if loop.index0 > ns.last_query_index %}\n            {%- if loop.last or (not loop.last and reasoning_content) %}\n                {{- '<|im_start|>' + message.role + '\\n<think>\\n' + reasoning_content.strip('\\n') + '\\n</think>\\n\\n' + content.lstrip('\\n') }}\n            {%- else %}\n                {{- '<|im_start|>' + message.role + '\\n' + content }}\n            {%- endif %}\n        {%- else %}\n            {{- '<|im_start|>' + message.role + '\\n' + content }}\n        {%- endif %}\n        {%- if message.tool_calls %}\n            {%- for tool_call in message.tool_calls %}\n                {%- if (loop.first and content) or (not loop.first) %}\n                    {{- '\\n' }}\n                {%- endif %}\n                {%- if tool_call.function %}\n                    {%- set tool_call = tool_call.function %}\n                {%- endif %}\n                {{- '<tool_call>\\n{\"name\": \"' }}\n                {{- tool_call.name }}\n                {{- '\", \"arguments\": ' }}\n                {%- if tool_call.arguments is string %}\n                    {{- tool_call.arguments }}\n                {%- else %}\n                    {{- tool_call.arguments | tojson }}\n                {%- endif %}\n                {{- '}\\n</tool_call>' }}\n            {%- endfor %}\n        {%- endif %}\n        {{- '<|im_end|>\\n' }}\n    {%- elif message.role == \"tool\" %}\n        {%- if loop.first or (messages[loop.index0 - 1].role != \"tool\") %}\n            {{- '<|im_start|>user' }}\n        {%- endif %}\n        {{- '\\n<tool_response>\\n' }}\n        {{- message.content }}\n        {{- '\\n</tool_response>' }}\n        {%- if loop.last or (messages[loop.index0 + 1].role != \"tool\") %}\n            {{- '<|im_end|>\\n' }}\n        {%- endif %}\n    {%- endif %}\n{%- endfor %}\n{%- if add_generation_prompt %}\n    {{- '<|im_start|>assistant\\n' }}\n    {%- if enable_thinking is defined and enable_thinking is false %}\n        {{- '<think>\\n\\n</think>\\n\\n' }}\n    {%- endif %}\n{%- endif %}";
        let mut chatstate = ChatState::new(template.into(), "".into(), "".into(), vec![]);

        // Test 1: First message - establishes baseline cache
        chatstate.add_user_message("Hi, robot!".into());
        let (prefix_index1, diff1) = chatstate.find_token_diff_and_prefix_index(&model).unwrap();

        assert_eq!(prefix_index1, 0, "First call should have no prefix cached");
        assert!(
            diff1.len() > 0,
            "First call should return all tokens as diff"
        );

        let initial_cache_size = chatstate.prefix_cache.len();
        println!("Initial cache size: {}", initial_cache_size);

        // Test 2: Add assistant message with thinking - should find common prefix
        chatstate.add_assistant_message("<think>\nHm... That's a tough cookie. I think the answer is probably 42.\nCould it be something else?\nNah... It's 42!\n</think>\nThe answer is 42!".into());
        let (prefix_index2, diff2) = chatstate.find_token_diff_and_prefix_index(&model).unwrap();

        assert!(
            prefix_index2 > 0,
            "Should find some common prefix from user message"
        );
        assert!(
            diff2.len() > 0,
            "Should have new tokens for assistant message"
        );
        assert!(
            prefix_index2 < chatstate.prefix_cache.len() as u32,
            "Prefix should be less than total cache"
        );

        println!(
            "After assistant message: prefix_index={}, diff_len={}, total_cache={}",
            prefix_index2,
            diff2.len(),
            chatstate.prefix_cache.len()
        );

        // Test 3: Same messages again - should return empty diff
        let (prefix_index3, diff3) = chatstate.find_token_diff_and_prefix_index(&model).unwrap();

        assert_eq!(
            diff3.len(),
            0,
            "Identical template render should produce empty diff"
        );
        assert_eq!(
            prefix_index3,
            chatstate.prefix_cache.len() as u32,
            "Should match entire cache"
        );

        println!(
            "Same render: prefix_index={}, diff_len={}",
            prefix_index3,
            diff3.len()
        );

        // Test 4: Add another user message - should reuse existing conversation prefix
        chatstate.add_user_message("What are you on about? I need the real answer.".into());
        let (prefix_index4, diff4) = chatstate.find_token_diff_and_prefix_index(&model).unwrap();

        assert!(prefix_index4 > 0, "Should reuse some conversation history");
        assert!(
            diff4.len() > 0,
            "Should have tokens for new user message + generation prompt"
        );
        assert!(
            prefix_index4 < chatstate.prefix_cache.len() as u32,
            "Should not match entire cache"
        );

        println!(
            "New user message: prefix_index={}, diff_len={}, total_cache={}",
            prefix_index4,
            diff4.len(),
            chatstate.prefix_cache.len()
        );

        // Test 5: Add final assistant message - comprehensive conversation test
        chatstate.add_assistant_message("<think>\nI already told the user that the real answer is 42.\nI guess I'll just tell them again. What an idiot...\n</think>\nThe answer is 42!".into());
        let (prefix_index5, diff5) = chatstate.find_token_diff_and_prefix_index(&model).unwrap();

        assert!(prefix_index5 > 0, "Should reuse conversation history");
        assert!(
            diff5.len() > 0,
            "Should have tokens for final assistant response"
        );

        println!(
            "Final assistant message: prefix_index={}, diff_len={}, total_cache={}",
            prefix_index5,
            diff5.len(),
            chatstate.prefix_cache.len()
        );

        // Verify cache consistency
        assert!(
            chatstate.prefix_cache.len() > initial_cache_size,
            "Cache should have grown with conversation"
        );
    }
}
