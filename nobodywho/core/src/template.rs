use std::collections::HashMap;
use std::sync::LazyLock;

use minijinja::{Environment, Template, Value};
use tracing::{debug, trace, warn};

use crate::{
    chat::{Message, Role},
    errors::SelectTemplateError,
    tool_calling::Tool,
};

fn strftime_now(format_str: &str) -> String {
    chrono::Local::now().format(format_str).to_string()
}

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

pub struct ChatTemplate {
    template: String,
    bos_token: String,
    eos_token: String,
    /// Variables that the template references (detected at load time)
    undeclared_variables: Vec<String>,
}

impl ChatTemplate {
    pub fn new(
        original_template: &str,
        bos_token: &str,
        eos_token: &str,
    ) -> Result<Self, minijinja::Error> {
        let template = MINIJINJA_ENV.template_from_str(original_template)?;

        trace!("Loading chat template: {}", original_template);

        let undeclared_variables = template
            .undeclared_variables(true)
            .into_iter()
            .collect::<Vec<String>>();

        Ok(Self {
            template: original_template.to_string(),
            bos_token: bos_token.to_string(),
            eos_token: eos_token.to_string(),
            undeclared_variables,
        })
    }

    /// Warn if any template variables are set but not used by the template.
    fn warn_unused_template_variables(&self, ctx: &ChatTemplateContext) {
        for key in ctx.template_variables.keys() {
            if !self.undeclared_variables.contains(key) {
                warn!(
                    "Template variable '{}' is set but the template does not use it. This setting will have no effect.",
                    key
                );
            }
        }
    }

    fn get_template(&self) -> Result<Template<'_, '_>, minijinja::Error> {
        MINIJINJA_ENV.template_from_str(&self.template)
    }

    pub fn render_unhandled(
        &self,
        messages: &[Message],
        ctx: &ChatTemplateContext,
    ) -> Result<String, minijinja::Error> {
        // Warn about any template variables that won't have effect
        self.warn_unused_template_variables(ctx);

        let add_generation_prompt = messages.last().is_some_and(|msg| {
            matches!(
                msg,
                Message::Message {
                    role: Role::User,
                    ..
                } | Message::ToolResp { .. }
            )
        });

        let template = self.get_template()?;

        // Build context with base variables
        let mut context_map: HashMap<&str, Value> = [
            ("messages", Value::from_serialize(messages)),
            ("add_generation_prompt", Value::from(add_generation_prompt)),
            ("bos_token", Value::from(&self.bos_token)),
            ("eos_token", Value::from(&self.eos_token)),
            ("tools", Value::from_serialize(&ctx.tools)),
        ]
        .into_iter()
        .collect();

        // Merge user-provided template variables
        for (key, value) in &ctx.template_variables {
            context_map.insert(key.as_str(), Value::from(*value));
        }

        template.render(Value::from_iter(context_map))
    }

    /// given a chat history where the first two messages are from system and user
    /// return a history where the first message is from user, and contains the system prompt as well.
    /// (this is what llama.cpp does for the gemma template too)
    fn concat_system_and_first_user_messages(
        &self,
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
                // HACK: this should probably be a custom error, and not a minijinja error
                //       but this was quick and easy rn, and we "abuse" the minijinja errors for
                //       `raise_exception` anyway...
                Err(minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    "Cannot replace system prompt unless the first two messages are from system and user roles."
                ))
            }
        }
    }

    pub fn render(
        &self,
        messages: &[Message],
        ctx: &ChatTemplateContext,
    ) -> Result<String, minijinja::Error> {
        let rendered_template = self.render_unhandled(messages, ctx);
        let result = match rendered_template {
            Ok(rendered) => Ok(rendered),
            Err(err) => match err.kind() {
                minijinja::ErrorKind::InvalidOperation
                    if err.to_string().contains("System role not supported") =>
                {
                    debug!("Concatenating first user messages. System role not supported");
                    // this is the error message we get when rendering the gemma2 template
                    // concat the first two messages and try again
                    self.render_unhandled(
                        &self.concat_system_and_first_user_messages(messages)?,
                        ctx,
                    )
                }
                minijinja::ErrorKind::InvalidOperation
                    if err.to_string().contains(
                        "Conversation roles must alternate user/assistant/user/assistant/...",
                    ) =>
                {
                    // this is the error we get when rendering the mistral 7b v0.3 template,
                    // which, like gemma2, does not support the system role
                    // concat the first two messages and try again
                    debug!("Concatenating first user messages. Conversation roles must alternate");
                    self.render_unhandled(
                        &self.concat_system_and_first_user_messages(messages)?,
                        ctx,
                    )
                }
                _ => {
                    debug!(error = %err, "Template render failed with InvalidOperation:");
                    Err(err)
                }
            },
        };

        let text = result?;
        trace!(%text, "Rendered template:\n");

        Ok(text)
    }
}

pub struct ChatTemplateContext {
    /// Custom template variables (e.g., {"enable_thinking": false})
    pub template_variables: HashMap<String, bool>,
    pub tools: Option<Vec<Tool>>,
}

pub fn select_template(
    model: &llama_cpp_2::model::LlamaModel,
    with_tools: bool,
) -> Result<ChatTemplate, SelectTemplateError> {
    let default_template = model.chat_template(None)?.to_string()?;
    let tool_template = model.chat_template(Some("tool_use"));
    let bos = model.token_to_piece(
        model.token_bos(),
        &mut encoding_rs::UTF_8.new_decoder(),
        true,
        None,
    )?;
    let eos = model.token_to_piece(
        model.token_eos(),
        &mut encoding_rs::UTF_8.new_decoder(),
        true,
        None,
    )?;

    let template = if !with_tools {
        // no tools. use default template.
        debug!("Selecting default template, no tools provided");
        default_template
    } else if let Ok(tool_template) = tool_template {
        // tools provided, and we have a tool template, use that.
        debug_assert!(tool_template.to_string()?.contains("tools"));
        debug!("Selecting tool template, tools provided");
        tool_template.to_string()?
    } else if default_template.contains("tools") {
        // tools provided, but no tool template, but the default template seems to mention tools
        debug!("Selecting default template with tool support, tools provided");
        default_template
    } else {
        // tools provided, but we have no tool-capable template
        return Err(SelectTemplateError::NoToolTemplate);
    };

    Ok(ChatTemplate::new(&template, &bos, &eos)?)
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

        let bos = "<|begin_of_text|>";
        let eos = "<|end_of_text|>";
        let ctx = ChatTemplateContext {
            template_variables: HashMap::new(),
            tools: None,
        };

        let chat_template = ChatTemplate::new(template, bos, eos).unwrap();

        // Test 1: Single user message
        let mut messages = vec![Message::Message {
            role: Role::User,
            content: "Hello, world!".into(),
        }];
        let rendered = chat_template.render(&messages, &ctx).unwrap();

        let expected = "<|begin_of_text|><|start_header_id|>user<|end_header_id|>\n\nHello, world!<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n";
        assert_eq!(rendered, expected);

        // Test 2: Add assistant response
        messages.push(Message::Message {
            role: Role::Assistant,
            content: "Hi there! How can I help?".into(),
        });
        let rendered2 = chat_template.render(&messages, &ctx).unwrap();

        let expected2 = "<|begin_of_text|><|start_header_id|>user<|end_header_id|>\n\nHello, world!<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\nHi there! How can I help?<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n";
        assert_eq!(rendered2, expected2);

        // Test 3: Multi-turn conversation
        messages.push(Message::Message {
            role: Role::User,
            content: "What's the weather like?".into(),
        });
        messages.push(Message::Message {
            role: Role::Assistant,
            content: "I don't have access to weather data.".into(),
        });
        let rendered3 = chat_template.render(&messages, &ctx).unwrap();

        assert!(rendered3.starts_with(
            "<|begin_of_text|><|start_header_id|>user<|end_header_id|>\n\nHello, world!<|eot_id|>"
        ));
        assert!(rendered3.contains(
            "<|start_header_id|>user<|end_header_id|>\n\nWhat's the weather like?<|eot_id|>"
        ));
        assert!(rendered3.contains("<|start_header_id|>assistant<|end_header_id|>\n\nI don't have access to weather data.<|eot_id|>"));
        assert!(rendered3.ends_with("<|start_header_id|>assistant<|end_header_id|>\n\n"));

        // Test 4: System message (if added first)
        let messages = vec![
            Message::Message {
                role: Role::System,
                content: "You are a helpful assistant.".into(),
            },
            Message::Message {
                role: Role::User,
                content: "Hi".into(),
            },
        ];
        let rendered4 = chat_template.render(&messages, &ctx).unwrap();

        println!("{:?}", rendered4);

        assert!(rendered4.starts_with("<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\nYou are a helpful assistant.<|eot_id|>"));
        assert!(rendered4.contains("<|start_header_id|>user<|end_header_id|>\n\nHi<|eot_id|>"));
        assert!(rendered4.ends_with("<|start_header_id|>assistant<|end_header_id|>\n\n"));
    }

    #[test]
    fn test_render_string_deepseek_template() {
        // DeepSeek template from the existing test
        let template = "{% if not add_generation_prompt is defined %}{% set add_generation_prompt = false %}{% endif %}{% set ns = namespace(is_first=false, is_tool=false, is_output_first=true, system_prompt='') %}{%- for message in messages %}{%- if message['role'] == 'system' %}{% set ns.system_prompt = message['content'] %}{%- endif %}{%- endfor %}{{bos_token}}{{ns.system_prompt}}{%- for message in messages %}{%- if message['role'] == 'user' %}{%- set ns.is_tool = false -%}{{'<｜User｜>' + message['content']}}{%- endif %}{%- if message['role'] == 'assistant' and message['content'] is none %}{%- set ns.is_tool = false -%}{%- for tool in message['tool_calls']%}{%- if not ns.is_first %}{{'<｜Assistant｜><｜tool▁calls▁begin｜><｜tool▁call▁begin｜>' + tool['type'] + '<｜tool▁sep｜>' + tool['function']['name'] + '\\n' + '```json' + '\\n' + tool['function']['arguments'] + '\\n' + '```' + '<｜tool▁call▁end｜>'}}{%- set ns.is_first = true -%}{%- else %}{{'\\n' + '<｜tool▁call▁begin｜>' + tool['type'] + '<｜tool▁sep｜>' + tool['function']['name'] + '\\n' + '```json' + '\\n' + tool['function']['arguments'] + '\\n' + '```' + '<｜tool▁call▁end｜>'}}{{'<｜tool▁calls▁end｜><｜end▁of▁sentence｜>'}}{%- endif %}{%- endfor %}{%- endif %}{%- if message['role'] == 'assistant' and message['content'] is not none %}{%- if ns.is_tool %}{{'<｜tool▁outputs▁end｜>' + message['content'] + '<｜end▁of▁sentence｜>'}}{%- set ns.is_tool = false -%}{%- else %}{% set content = message['content'] %}{% if '</think>' in content %}{% set content = content.split('</think>')[-1] %}{% endif %}{{'<｜Assistant｜>' + content + '<｜end▁of▁sentence｜>'}}{%- endif %}{%- endif %}{%- if message['role'] == 'tool' %}{%- set ns.is_tool = true -%}{%- if ns.is_output_first %}{{'<｜tool▁outputs▁begin｜><｜tool▁output▁begin｜>' + message['content'] + '<｜tool▁output▁end｜>'}}{%- set ns.is_output_first = false %}{%- else %}{{'\\n<｜tool▁output▁begin｜>' + message['content'] + '<｜tool▁output▁end｜>'}}{%- endif %}{%- endif %}{%- endfor -%}{% if ns.is_tool %}{{'<｜tool▁outputs▁end｜>'}}{% endif %}{% if add_generation_prompt and not ns.is_tool %}{{'<｜Assistant｜>'}}{% endif %}";

        let bos = "<|bos|>";
        let eos = "<|eos|>";

        let ctx = ChatTemplateContext {
            template_variables: HashMap::new(),
            tools: None,
        };

        let chat_template = ChatTemplate::new(template, bos, eos).unwrap();

        // Test 1: Single user message
        let mut messages = vec![Message::Message {
            role: Role::User,
            content: "Hello, world!".into(),
        }];
        let rendered = chat_template.render(&messages, &ctx).unwrap();

        // render_string sets add_generation_prompt to true for user messages, so <｜Assistant｜> is added
        let expected = "<|bos|><｜User｜>Hello, world!<｜Assistant｜>";
        assert_eq!(rendered, expected);

        // Test 2: Add assistant response
        messages.push(Message::Message {
            role: Role::Assistant,
            content: "Hi there! How can I help?".into(),
        });
        let rendered2 = chat_template.render(&messages, &ctx).unwrap();

        let expected2 = "<|bos|><｜User｜>Hello, world!<｜Assistant｜>Hi there! How can I help?<｜end▁of▁sentence｜>";
        assert_eq!(rendered2, expected2);

        // Test 3: Assistant message with thinking block
        messages.push(Message::Message {
            role: Role::User,
            content: "Can you help me?".into(),
        });
        messages.push(Message::Message {
            role: Role::Assistant,
            content: "<think>The user is asking for help</think>I'd be happy to assist you!".into(),
        });
        let rendered3 = chat_template.render(&messages, &ctx).unwrap();

        // The thinking block should be stripped out, only the content after </think> should remain
        assert!(
            rendered3.contains("<｜Assistant｜>I'd be happy to assist you!<｜end▁of▁sentence｜>")
        );
        assert!(!rendered3.contains("<think>"));
        assert!(!rendered3.contains("</think>"));

        // Test 4: System message
        let messages = vec![
            Message::Message {
                role: Role::System,
                content: "You are a helpful assistant.".into(),
            },
            Message::Message {
                role: Role::User,
                content: "Hi".into(),
            },
        ];
        let rendered4 = chat_template.render(&messages, &ctx).unwrap();

        let expected4 = "<|bos|>You are a helpful assistant.<｜User｜>Hi<｜Assistant｜>";
        assert_eq!(rendered4, expected4);

        // Test 5: Multi-turn conversation
        let messages = vec![
            Message::Message {
                role: Role::User,
                content: "What's 2+2?".into(),
            },
            Message::Message {
                role: Role::Assistant,
                content: "4".into(),
            },
            Message::Message {
                role: Role::User,
                content: "Thanks!".into(),
            },
        ];
        let rendered5 = chat_template.render(&messages, &ctx).unwrap();

        let expected5 =
            "<|bos|><｜User｜>What's 2+2?<｜Assistant｜>4<｜end▁of▁sentence｜><｜User｜>Thanks!<｜Assistant｜>";
        assert_eq!(rendered5, expected5);

        // Test 6: Empty messages (no generation prompt by default)
        let messages: Vec<Message> = vec![];
        let rendered6 = chat_template.render(&messages, &ctx).unwrap();

        let expected6 = "<|bos|>";
        assert_eq!(rendered6, expected6);
    }

    #[test]
    fn test_render_string_qwen3_template() {
        // Qwen3 template from the existing test
        let template = "{%- if tools %}\n    {{- '<|im_start|>system\\n' }}\n    {%- if messages[0].role == 'system' %}\n        {{- messages[0].content + '\\n\\n' }}\n    {%- endif %}\n    {{- \"# Tools\\n\\nYou may call one or more functions to assist with the user query.\\n\\nYou are provided with function signatures within <tools></tools> XML tags:\\n<tools>\" }}\n    {%- for tool in tools %}\n        {{- \"\\n\" }}\n        {{- tool | tojson }}\n    {%- endfor %}\n    {{- \"\\n</tools>\\n\\nFor each function call, return a json object with function name and arguments within <tool_call></tool_call> XML tags:\\n<tool_call>\\n{\\\"name\\\": <function-name>, \\\"arguments\\\": <args-json-object>}\\n</tool_call><|im_end|>\\n\" }}\n{%- else %}\n    {%- if messages[0].role == 'system' %}\n        {{- '<|im_start|>system\\n' + messages[0].content + '<|im_end|>\\n' }}\n    {%- endif %}\n{%- endif %}\n{%- set ns = namespace(multi_step_tool=true, last_query_index=messages|length - 1) %}\n{%- for message in messages[::-1] %}\n    {%- set index = (messages|length - 1) - loop.index0 %}\n    {%- if ns.multi_step_tool and message.role == \"user\" and not(message.content.startswith('<tool_response>') and message.content.endswith('</tool_response>')) %}\n        {%- set ns.multi_step_tool = false %}\n        {%- set ns.last_query_index = index %}\n    {%- endif %}\n{%- endfor %}\n{%- for message in messages %}\n    {%- if (message.role == \"user\") or (message.role == \"system\" and not loop.first) %}\n        {{- '<|im_start|>' + message.role + '\\n' + message.content + '<|im_end|>' + '\\n' }}\n    {%- elif message.role == \"assistant\" %}\n        {%- set content = message.content %}\n        {%- set reasoning_content = '' %}\n        {%- if message.reasoning_content is defined and message.reasoning_content is not none %}\n            {%- set reasoning_content = message.reasoning_content %}\n        {%- else %}\n            {%- if '</think>' in message.content %}\n                {%- set content = message.content.split('</think>')[-1].lstrip('\\n') %}\n                {%- set reasoning_content = message.content.split('</think>')[0].rstrip('\\n').split('<think>')[-1].lstrip('\\n') %}\n            {%- endif %}\n        {%- endif %}\n        {%- if loop.index0 > ns.last_query_index %}\n            {%- if loop.last or (not loop.last and reasoning_content) %}\n                {{- '<|im_start|>' + message.role + '\\n<think>\\n' + reasoning_content.strip('\\n') + '\\n</think>\\n\\n' + content.lstrip('\\n') }}\n            {%- else %}\n                {{- '<|im_start|>' + message.role + '\\n' + content }}\n            {%- endif %}\n        {%- else %}\n            {{- '<|im_start|>' + message.role + '\\n' + content }}\n        {%- endif %}\n        {%- if message.tool_calls %}\n            {%- for tool_call in message.tool_calls %}\n                {%- if (loop.first and content) or (not loop.first) %}\n                    {{- '\\n' }}\n                {%- endif %}\n                {%- if tool_call.function %}\n                    {%- set tool_call = tool_call.function %}\n                {%- endif %}\n                {{- '<tool_call>\\n{\"name\": \"' }}\n                {{- tool_call.name }}\n                {{- '\", \"arguments\": ' }}\n                {%- if tool_call.arguments is string %}\n                    {{- tool_call.arguments }}\n                {%- else %}\n                    {{- tool_call.arguments | tojson }}\n                {%- endif %}\n                {{- '}\\n</tool_call>' }}\n            {%- endfor %}\n        {%- endif %}\n        {{- '<|im_end|>\\n' }}\n    {%- elif message.role == \"tool\" %}\n        {%- if loop.first or (messages[loop.index0 - 1].role != \"tool\") %}\n            {{- '<|im_start|>user' }}\n        {%- endif %}\n        {{- '\\n<tool_response>\\n' }}\n        {{- message.content }}\n        {{- '\\n</tool_response>' }}\n        {%- if loop.last or (messages[loop.index0 + 1].role != \"tool\") %}\n            {{- '<|im_end|>\\n' }}\n        {%- endif %}\n    {%- endif %}\n{%- endfor %}\n{%- if add_generation_prompt %}\n    {{- '<|im_start|>assistant\\n' }}\n    {%- if enable_thinking is defined and enable_thinking is false %}\n        {{- '<think>\\n\\n</think>\\n\\n' }}\n    {%- endif %}\n{%- endif %}";

        let bos = "";
        let eos = "";

        let ctx = ChatTemplateContext {
            template_variables: HashMap::new(),
            tools: None,
        };
        let chat_template = ChatTemplate::new(template, bos, eos).unwrap();

        // Test 1: Single user message
        let mut messages = vec![Message::Message {
            role: Role::User,
            content: "Hi, robot!".into(),
        }];
        let rendered = chat_template.render(&messages, &ctx).unwrap();

        let expected = "<|im_start|>user\nHi, robot!<|im_end|>\n<|im_start|>assistant\n";
        assert_eq!(rendered, expected);

        // Test 2: Add assistant response with thinking
        messages.push(Message::Message {
            role: Role::Assistant,
            content: "<think>\nHm... That's a tough cookie. I think the answer is probably 42.\nCould it be something else?\nNah... It's 42!\n</think>\nThe answer is 42!".into(),
        });
        let rendered2 = chat_template.render(&messages, &ctx).unwrap();

        // The thinking block should be included in the output for Qwen3
        let expected2 = "<|im_start|>user\nHi, robot!<|im_end|>\n<|im_start|>assistant\n<think>\nHm... That's a tough cookie. I think the answer is probably 42.\nCould it be something else?\nNah... It's 42!\n</think>\n\nThe answer is 42!<|im_end|>\n";
        assert_eq!(rendered2, expected2);

        // Test 3: System message
        let messages = vec![
            Message::Message {
                role: Role::System,
                content: "You are a helpful assistant.".into(),
            },
            Message::Message {
                role: Role::User,
                content: "Hello".into(),
            },
        ];
        let rendered3 = chat_template.render(&messages, &ctx).unwrap();

        let expected3 = "<|im_start|>system\nYou are a helpful assistant.<|im_end|>\n<|im_start|>user\nHello<|im_end|>\n<|im_start|>assistant\n";
        assert_eq!(rendered3, expected3);

        // Test 4: Multi-turn conversation
        let messages = vec![
            Message::Message {
                role: Role::User,
                content: "What's 2+2?".into(),
            },
            Message::Message {
                role: Role::Assistant,
                content: "4".into(),
            },
            Message::Message {
                role: Role::User,
                content: "Thanks!".into(),
            },
        ];
        let rendered4 = chat_template.render(&messages, &ctx).unwrap();

        let expected4 = "<|im_start|>user\nWhat's 2+2?<|im_end|>\n<|im_start|>assistant\n4<|im_end|>\n<|im_start|>user\nThanks!<|im_end|>\n<|im_start|>assistant\n";
        assert_eq!(rendered4, expected4);

        // Test 5: Assistant message without thinking
        let messages = vec![
            Message::Message {
                role: Role::User,
                content: "Hello".into(),
            },
            Message::Message {
                role: Role::Assistant,
                content: "Hi there!".into(),
            },
        ];
        let rendered5 = chat_template.render(&messages, &ctx).unwrap();

        // The template now includes empty thinking blocks for assistant messages
        let expected5 = "<|im_start|>user\nHello<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\nHi there!<|im_end|>\n";
        assert_eq!(rendered5, expected5);
    }
}
