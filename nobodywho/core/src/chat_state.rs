use std::sync::LazyLock;

use minijinja::{context, Environment};
use serde::{self, Serialize};

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

#[derive(Serialize, Clone, Debug)]
pub struct Message {
    pub role: String,
    pub content: String,
}

pub struct ChatState {
    messages: Vec<Message>,
    chat_template: String,
    length: usize,
    eos_token: String,
    bos_token: String,
}

/// given a chat history where the first two messages are from system and user
/// return a history where the first message is from user, and contains the system prompt as well.
/// (this is what llama.cpp does for the gemma template too)
fn concat_system_and_first_user_messages(
    messages: &[Message],
) -> Result<Vec<Message>, minijinja::Error> {
    if messages.len() < 2 || messages[0].role != "system" || messages[1].role != "user" {
        // HACK: this should probably be a custom ChatStateError, and nont a minijinja error
        //       but this was quick and easy rn, and we "abuse" the minijinja errors for
        //       `raise_exception` anyway...
        return Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "Cannot replace system prompt unless the first two messages are from system and user roles."
        ));
    }
    let new_first_message = Message {
        role: "user".to_string(),
        content: format!("{}\n\n{}", messages[0].content, messages[1].content),
    };
    let new_messages = vec![new_first_message]
        .into_iter()
        .chain(messages[2..].iter().cloned())
        .collect();
    Ok(new_messages)
}

#[derive(Debug, thiserror::Error)]
pub enum FromModelError {
    #[error("Lama.cpp failed fetching chat template from the model file. This is likely because you're using an older GGUF file, which might not include a chat template. For example, this is the case for most LLaMA2-based GGUF files. Try using a more recent GGUF model file. If you want to check if a given model includes a chat template, you can use the gguf-dump script from llama.cpp. Here is a more technical detailed error: {0}")]
    ChatTemplateError(#[from] llama_cpp_2::ChatTemplateError),

    #[error("Could not parse chat template as UTF8: {0}")]
    TemplateUtf8Error(#[from] std::str::Utf8Error),

    #[error("Could not detokenize string: {0}")]
    Detokenize(#[from] llama_cpp_2::TokenToStringError),
}

impl ChatState {
    pub fn new(chat_template: String, bos_token: String, eos_token: String) -> Self {
        Self {
            messages: Vec::new(),
            chat_template,
            length: 0,
            eos_token,
            bos_token,
        }
    }

    pub fn from_model(model: &llama_cpp_2::model::LlamaModel) -> Result<Self, FromModelError> {
        let template = model.get_chat_template()?.to_string()?;
        let tokenize = llama_cpp_2::model::Special::Tokenize;
        let bos = model.token_to_str(model.token_bos(), tokenize)?;
        let eos = model.token_to_str(model.token_eos(), tokenize)?;
        Ok(Self::new(template, bos, eos))
    }

    pub fn add_message(&mut self, role: String, content: String) {
        self.messages.push(Message { role, content });
    }

    fn render(&mut self) -> Result<String, minijinja::Error> {
        let tmpl = MINIJINJA_ENV.template_from_str(&self.chat_template)?;

        let ctx = context! {
            messages => &self.messages,
            add_generation_prompt => self.messages.last().map_or(false, |msg| msg.role == "user"),
            eos_token => self.eos_token,
            bos_token => self.bos_token,
        };

        match tmpl.render(ctx) {
            Ok(rendered) => Ok(rendered),
            Err(err) => match err.kind() {
                minijinja::ErrorKind::InvalidOperation => {
                    if err.to_string().contains("System role not supported") {
                        // this is the error message we get when rendering the gemma2 template
                        // concat the first two messages and try again
                        self.messages = concat_system_and_first_user_messages(&self.messages)?;
                        self.render()
                    } else if err.to_string().contains(
                        "Conversation roles must alternate user/assistant/user/assistant/...",
                    ) {
                        // this is the error we get when rendering the mistral 7b v0.3 template,
                        // which, like gemma2, does not support the system role
                        // concat the first two messages and try again
                        self.messages = concat_system_and_first_user_messages(&self.messages)?;
                        self.render()
                    } else {
                        Err(err)
                    }
                }
                _ => Err(err),
            },
        }
    }

    pub fn render_diff(&mut self) -> Result<String, minijinja::Error> {
        // render the full template
        let text = self.render()?;

        // get the chars that are new since the last template render
        let diff = text[self.length..].to_string();

        // note the length of this template render
        self.length = text.len();

        Ok(diff)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llama31_template() {
        // test that llama 3.1 template renders
        let template = "{% set loop_messages = messages %}{% for message in loop_messages %}{% set content = '<|start_header_id|>' + message['role'] + '<|end_header_id|>\n\n'+ message['content'] | trim + '<|eot_id|>' %}{% if loop.index0 == 0 %}{% set content = bos_token + content %}{% endif %}{{ content }}{% endfor %}{{ '<|start_header_id|>assistant<|end_header_id|>\n\n' }}";
        let mut chatstate = ChatState::new(template.into(), "<|bos|>".into(), "<|eos|>".into());
        chatstate.add_message("user".into(), "Hello, world!".into());
        let rendered = chatstate.render_diff().unwrap();
        let expected = "<|bos|><|start_header_id|>user<|end_header_id|>

Hello, world!<|eot_id|><|start_header_id|>assistant<|end_header_id|>

";
        assert_eq!(rendered, expected)
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
    fn test_deepseek_template() {
        let template = "{% if not add_generation_prompt is defined %}{% set add_generation_prompt = false %}{% endif %}{% set ns = namespace(is_first=false, is_tool=false, is_output_first=true, system_prompt='') %}{%- for message in messages %}{%- if message['role'] == 'system' %}{% set ns.system_prompt = message['content'] %}{%- endif %}{%- endfor %}{{bos_token}}{{ns.system_prompt}}{%- for message in messages %}{%- if message['role'] == 'user' %}{%- set ns.is_tool = false -%}{{'<｜User｜>' + message['content']}}{%- endif %}{%- if message['role'] == 'assistant' and message['content'] is none %}{%- set ns.is_tool = false -%}{%- for tool in message['tool_calls']%}{%- if not ns.is_first %}{{'<｜Assistant｜><｜tool▁calls▁begin｜><｜tool▁call▁begin｜>' + tool['type'] + '<｜tool▁sep｜>' + tool['function']['name'] + '\\n' + '```json' + '\\n' + tool['function']['arguments'] + '\\n' + '```' + '<｜tool▁call▁end｜>'}}{%- set ns.is_first = true -%}{%- else %}{{'\\n' + '<｜tool▁call▁begin｜>' + tool['type'] + '<｜tool▁sep｜>' + tool['function']['name'] + '\\n' + '```json' + '\\n' + tool['function']['arguments'] + '\\n' + '```' + '<｜tool▁call▁end｜>'}}{{'<｜tool▁calls▁end｜><｜end▁of▁sentence｜>'}}{%- endif %}{%- endfor %}{%- endif %}{%- if message['role'] == 'assistant' and message['content'] is not none %}{%- if ns.is_tool %}{{'<｜tool▁outputs▁end｜>' + message['content'] + '<｜end▁of▁sentence｜>'}}{%- set ns.is_tool = false -%}{%- else %}{% set content = message['content'] %}{% if '</think>' in content %}{% set content = content.split('</think>')[-1] %}{% endif %}{{'<｜Assistant｜>' + content + '<｜end▁of▁sentence｜>'}}{%- endif %}{%- endif %}{%- if message['role'] == 'tool' %}{%- set ns.is_tool = true -%}{%- if ns.is_output_first %}{{'<｜tool▁outputs▁begin｜><｜tool▁output▁begin｜>' + message['content'] + '<｜tool▁output▁end｜>'}}{%- set ns.is_output_first = false %}{%- else %}{{'\\n<｜tool▁output▁begin｜>' + message['content'] + '<｜tool▁output▁end｜>'}}{%- endif %}{%- endif %}{%- endfor -%}{% if ns.is_tool %}{{'<｜tool▁outputs▁end｜>'}}{% endif %}{% if add_generation_prompt and not ns.is_tool %}{{'<｜Assistant｜>'}}{% endif %}";
        let mut chatstate = ChatState::new(template.into(), "<|bos|>".into(), "<|eos|>".into());
        chatstate.add_message("user".into(), "Hello, world!".into());
        chatstate.add_message(
            "assistant".into(),
            "<think>beep boop robot thinky</think>".into(),
        );
        let rendered = chatstate.render_diff();
        println!("{:?}", rendered);
        assert!(rendered.is_ok());
    }
}
