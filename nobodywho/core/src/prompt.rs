#[derive(Clone, Debug)]
pub struct Prompt {
    parts: Vec<PromptPart>,
}

impl Prompt {
    pub fn new() -> Self {
        Self { parts: vec![] }
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.parts.push(PromptPart::Text(text.into()));
        self
    }

    pub fn with_image(mut self, image_path: impl Into<String>) -> Self {
        self.parts.push(PromptPart::Image(image_path.into()));
        self
    }

    pub fn to_string(&self) -> String {
        let marker = llama_cpp_2::mtmd::mtmd_default_marker();
        self.parts
            .iter()
            .map(|part| match part {
                PromptPart::Text(text) => text.clone(),
                PromptPart::Image(_) => marker.to_string(),
            })
            .collect::<Vec<String>>()
            .join("")
    }

    pub fn extract_paths(&self) -> Vec<String> {
        self.parts
            .iter()
            .filter_map(|part| match part {
                PromptPart::Image(path) => Some(path.clone()),
                PromptPart::Text(_) => None,
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
enum PromptPart {
    Text(String),
    Image(String),
}

pub trait Promptable {
    fn to_prompt(&self) -> Prompt;
}

impl Promptable for String {
    fn to_prompt(&self) -> Prompt {
        Prompt {
            parts: vec![PromptPart::Text(self.clone())],
        }
    }
}

impl Promptable for Prompt {
    fn to_prompt(&self) -> Prompt {
        self.clone()
    }
}
