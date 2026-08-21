//! Focused ONNX Runtime prototype for DFM-Mimir and compatible HrmText models.
//!
//! The expected model is an Optimum-style merged causal-LM graph with explicit
//! `past_key_values.*` inputs and `present.*` outputs. Generation is greedy and
//! each request prefills the complete rendered conversation.

use crate::chat::Message;
use crate::errors::MimirError;
use crate::onnx::Device;
use crate::stream::{StreamOutput, TokenStream};
use crate::template::{ChatTemplate, ChatTemplateContext};
use half::f16;
use ort::memory::Allocator;
use ort::session::Session;
use ort::value::{DynValue, Tensor, TensorElementType, ValueType};
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc;
use tokenizers::Tokenizer;
use tracing::{debug, info};

const DEFAULT_MODEL_FILE: &str = "onnx/model_int8.onnx";
const DEFAULT_MAX_NEW_TOKENS: usize = 256;

#[derive(Clone, Debug)]
pub struct MimirConfig {
    pub source: String,
    pub model_file: String,
    pub max_new_tokens: usize,
    pub system_prompt: Option<String>,
    pub template_variables: HashMap<String, bool>,
}

impl MimirConfig {
    pub fn new(source: impl AsRef<str>) -> Self {
        Self {
            source: source.as_ref().to_string(),
            model_file: DEFAULT_MODEL_FILE.to_string(),
            max_new_tokens: DEFAULT_MAX_NEW_TOKENS,
            system_prompt: None,
            template_variables: HashMap::new(),
        }
    }
}

enum Request {
    Ask {
        prompt: String,
        output: tokio::sync::mpsc::UnboundedSender<StreamOutput<MimirError>>,
    },
    Reset(mpsc::Sender<()>),
}

/// Conversational HrmText inference backed by one ONNX Runtime worker.
#[derive(Clone)]
pub struct Mimir {
    request_tx: mpsc::Sender<Request>,
}

impl Mimir {
    pub fn new(config: MimirConfig) -> Result<Self, MimirError> {
        Self::with_device(config, Device::Auto)
    }

    pub fn with_device(config: MimirConfig, device: Device) -> Result<Self, MimirError> {
        let mut backend = MimirBackend::new(config, device)?;
        let (request_tx, request_rx) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(request) = request_rx.recv() {
                match request {
                    Request::Ask { prompt, output } => {
                        match backend.generate(&prompt, &mut |piece| {
                            let _ = output.send(StreamOutput::Token(piece));
                        }) {
                            Ok(text) => {
                                let _ = output.send(StreamOutput::Done(text));
                            }
                            Err(error) => {
                                let _ = output.send(StreamOutput::Error(error));
                            }
                        }
                    }
                    Request::Reset(done) => {
                        backend.reset();
                        let _ = done.send(());
                    }
                }
            }
        });
        Ok(Self { request_tx })
    }

    pub fn ask(&self, prompt: impl Into<String>) -> TokenStream<MimirError> {
        let (output_tx, output_rx) = tokio::sync::mpsc::unbounded_channel();
        if self
            .request_tx
            .send(Request::Ask {
                prompt: prompt.into(),
                output: output_tx.clone(),
            })
            .is_err()
        {
            let _ = output_tx.send(StreamOutput::Error(MimirError::WorkerDead));
        }
        TokenStream::new(output_rx)
    }

    pub fn reset(&self) -> Result<(), MimirError> {
        let (done_tx, done_rx) = mpsc::channel();
        self.request_tx
            .send(Request::Reset(done_tx))
            .map_err(|_| MimirError::WorkerDead)?;
        done_rx.recv().map_err(|_| MimirError::WorkerDead)
    }
}

struct CacheSpec {
    input_name: String,
    output_name: String,
    num_heads: usize,
    head_dim: usize,
    dtype: TensorElementType,
}

struct KVCache(Vec<(Cow<'static, str>, DynValue)>);

impl KVCache {
    fn empty(specs: &[CacheSpec]) -> Result<Self, MimirError> {
        let allocator = Allocator::default();
        let entries = specs
            .iter()
            .map(|spec| {
                let shape = [1_i64, spec.num_heads as i64, 0, spec.head_dim as i64];
                let value = match spec.dtype {
                    TensorElementType::Float16 => Tensor::<f16>::new(&allocator, shape)?.into_dyn(),
                    TensorElementType::Float32 => Tensor::<f32>::new(&allocator, shape)?.into_dyn(),
                    other => {
                        return Err(MimirError::Init(format!(
                            "unsupported KV-cache dtype {other}; expected float16 or float32"
                        )))
                    }
                };
                Ok((Cow::Owned(spec.input_name.clone()), value))
            })
            .collect::<Result<Vec<_>, MimirError>>()?;
        Ok(Self(entries))
    }
}

struct MimirBackend {
    session: Session,
    tokenizer: Tokenizer,
    template: ChatTemplate,
    template_context: ChatTemplateContext,
    history: Vec<Message>,
    system_prompt: Option<String>,
    cache_specs: Vec<CacheSpec>,
    eos_token_ids: Vec<u32>,
    max_context: usize,
    max_new_tokens: usize,
}

impl MimirBackend {
    fn new(config: MimirConfig, device: Device) -> Result<Self, MimirError> {
        if config.max_new_tokens == 0 {
            return Err(MimirError::Init(
                "max_new_tokens must be greater than zero".into(),
            ));
        }
        validate_model_file(&config.model_file)?;
        let model_dir = resolve_model_dir(&config.source, &config.model_file)?;
        let model_config = ModelConfig::from_dir(&model_dir)?;
        let tokenizer_config = TokenizerConfig::from_dir(&model_dir)?;
        let tokenizer = Tokenizer::from_file(model_dir.join("tokenizer.json"))
            .map_err(|error| MimirError::Tokenizer(error.to_string()))?;
        let template_source = std::fs::read_to_string(model_dir.join("chat_template.jinja"))?;
        let template = ChatTemplate::new(
            &template_source,
            &tokenizer_config.bos_token,
            &tokenizer_config.eos_token,
        )?;
        let session = crate::onnx::load_session(&model_dir.join(&config.model_file), device)?;
        let cache_specs = cache_specs_for(&session)?;
        validate_graph(&session, &cache_specs)?;

        info!(
            source = config.source,
            cache_slots = cache_specs.len() / 2,
            max_context = model_config.max_position_embeddings,
            "Loaded Mimir ONNX model"
        );

        Ok(Self {
            session,
            tokenizer,
            template,
            template_context: ChatTemplateContext::new(config.template_variables, None),
            history: Vec::new(),
            system_prompt: config.system_prompt.clone(),
            cache_specs,
            eos_token_ids: model_config.eos_token_id.into_vec(),
            max_context: model_config.max_position_embeddings,
            max_new_tokens: config.max_new_tokens,
        })
    }

    fn reset(&mut self) {
        self.history.clear();
    }

    fn messages_with(&self, prompt: &str) -> Vec<Message> {
        let mut messages = Vec::with_capacity(self.history.len() + 2);
        if let Some(system_prompt) = &self.system_prompt {
            messages.push(Message::new_system(system_prompt.clone()));
        }
        messages.extend(self.history.clone());
        messages.push(Message::new_user(prompt.to_string()));
        messages
    }

    fn generate(
        &mut self,
        prompt: &str,
        on_token: &mut dyn FnMut(String),
    ) -> Result<String, MimirError> {
        let messages = self.messages_with(prompt);
        let rendered = self.template.render(&messages, &self.template_context)?;
        let encoding = self
            .tokenizer
            .encode(rendered, false)
            .map_err(|error| MimirError::Tokenizer(error.to_string()))?;
        let prompt_tokens: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();
        if prompt_tokens.is_empty() {
            return Err(MimirError::Generation(
                "the rendered prompt produced no tokens".into(),
            ));
        }
        if prompt_tokens.len() >= self.max_context {
            return Err(MimirError::Generation(format!(
                "prompt is {} tokens but the model context is {}",
                prompt_tokens.len(),
                self.max_context
            )));
        }

        let max_new_tokens = self
            .max_new_tokens
            .min(self.max_context - prompt_tokens.len());
        let cache = KVCache::empty(&self.cache_specs)?;
        let token_types = vec![1_i64; prompt_tokens.len()];
        let (mut next_token, mut cache) = run_step(
            &mut self.session,
            &prompt_tokens,
            &token_types,
            prompt_tokens.len(),
            cache,
            self.cache_specs.len(),
        )?;

        let mut generated = Vec::new();
        let mut decode_stream = self.tokenizer.decode_stream(true);
        for step in 0..max_new_tokens {
            if self.eos_token_ids.contains(&(next_token as u32)) {
                break;
            }

            generated.push(next_token as u32);
            if let Some(piece) = decode_stream
                .step(next_token as u32)
                .map_err(|error| MimirError::Tokenizer(error.to_string()))?
            {
                on_token(piece);
            }
            debug!(step, next_token, "Mimir decode step");

            if step + 1 == max_new_tokens {
                break;
            }
            let total_length = prompt_tokens.len() + generated.len();
            (next_token, cache) = run_step(
                &mut self.session,
                &[next_token],
                &[0],
                total_length,
                cache,
                self.cache_specs.len(),
            )?;
        }

        let text = self
            .tokenizer
            .decode(&generated, true)
            .map_err(|error| MimirError::Tokenizer(error.to_string()))?;
        self.history.push(Message::new_user(prompt.to_string()));
        self.history.push(Message::new_assistant(text.clone()));
        info!(tokens = generated.len(), "Generated Mimir response");
        Ok(text)
    }
}

fn run_step(
    session: &mut Session,
    input_ids: &[i64],
    token_type_ids: &[i64],
    total_length: usize,
    cache: KVCache,
    expected_cache_values: usize,
) -> Result<(i64, KVCache), MimirError> {
    let sequence_length = input_ids.len();
    let mut inputs: Vec<(Cow<'static, str>, DynValue)> = vec![
        (
            "input_ids".into(),
            Tensor::from_array(([1_usize, sequence_length], input_ids.to_vec()))?.into_dyn(),
        ),
        (
            "attention_mask".into(),
            Tensor::from_array(([1_usize, total_length], vec![1_i64; total_length]))?.into_dyn(),
        ),
        (
            "token_type_ids".into(),
            Tensor::from_array(([1_usize, sequence_length], token_type_ids.to_vec()))?.into_dyn(),
        ),
        (
            "num_logits_to_keep".into(),
            Tensor::from_array(((), vec![1_i64]))?.into_dyn(),
        ),
    ];
    inputs.extend(cache.0);

    let outputs = session.run(inputs)?;
    let next_token = {
        let logits = outputs
            .get("logits")
            .ok_or_else(|| MimirError::Generation("ONNX graph returned no `logits`".into()))?;
        argmax(logits)?
    };

    let mut next_cache = Vec::with_capacity(expected_cache_values);
    for (name, value) in outputs {
        let Some(suffix) = name.strip_prefix("present.") else {
            continue;
        };
        next_cache.push((Cow::Owned(format!("past_key_values.{suffix}")), value));
    }
    if next_cache.len() != expected_cache_values {
        return Err(MimirError::Generation(format!(
            "ONNX graph returned {} cache values; expected {expected_cache_values}",
            next_cache.len()
        )));
    }
    Ok((next_token, KVCache(next_cache)))
}

fn argmax(logits: &DynValue) -> Result<i64, MimirError> {
    match logits.dtype().tensor_type() {
        Some(TensorElementType::Float16) => logits
            .try_extract_tensor::<f16>()?
            .1
            .iter()
            .enumerate()
            .max_by(|(_, left), (_, right)| left.to_f32().total_cmp(&right.to_f32()))
            .map(|(index, _)| index as i64)
            .ok_or_else(|| MimirError::Generation("ONNX graph returned empty logits".into())),
        Some(TensorElementType::Float32) => logits
            .try_extract_tensor::<f32>()?
            .1
            .iter()
            .enumerate()
            .max_by(|(_, left), (_, right)| left.total_cmp(right))
            .map(|(index, _)| index as i64)
            .ok_or_else(|| MimirError::Generation("ONNX graph returned empty logits".into())),
        dtype => Err(MimirError::Generation(format!(
            "unsupported logits dtype {dtype:?}; expected float16 or float32"
        ))),
    }
}

fn cache_specs_for(session: &Session) -> Result<Vec<CacheSpec>, MimirError> {
    let output_names: HashSet<&str> = session
        .outputs()
        .iter()
        .map(|output| output.name())
        .collect();
    let mut specs = Vec::new();
    for input in session
        .inputs()
        .iter()
        .filter(|input| input.name().starts_with("past_key_values."))
    {
        let suffix = input
            .name()
            .strip_prefix("past_key_values.")
            .expect("prefix checked");
        let output_name = format!("present.{suffix}");
        if !output_names.contains(output_name.as_str()) {
            return Err(MimirError::Init(format!(
                "ONNX graph is missing cache output {output_name:?}"
            )));
        }
        let ValueType::Tensor { ty, shape, .. } = input.dtype() else {
            return Err(MimirError::Init(format!(
                "cache input {:?} is not a tensor",
                input.name()
            )));
        };
        if shape.len() != 4 || shape[1] <= 0 || shape[3] <= 0 {
            return Err(MimirError::Init(format!(
                "cache input {:?} has unsupported shape {shape:?}",
                input.name()
            )));
        }
        specs.push(CacheSpec {
            input_name: input.name().to_string(),
            output_name,
            num_heads: shape[1] as usize,
            head_dim: shape[3] as usize,
            dtype: *ty,
        });
    }
    specs.sort_by_key(|spec| cache_sort_key(&spec.input_name));
    if specs.is_empty() || specs.len() % 2 != 0 {
        return Err(MimirError::Init(format!(
            "expected key/value cache inputs, found {}",
            specs.len()
        )));
    }
    Ok(specs)
}

fn cache_sort_key(name: &str) -> (usize, usize) {
    let mut parts = name.split('.');
    let _past = parts.next();
    let layer = parts
        .next()
        .and_then(|part| part.parse().ok())
        .unwrap_or(usize::MAX);
    let field = match parts.next() {
        Some("key") => 0,
        Some("value") => 1,
        _ => 2,
    };
    (layer, field)
}

fn validate_graph(session: &Session, cache_specs: &[CacheSpec]) -> Result<(), MimirError> {
    let inputs: HashSet<&str> = session.inputs().iter().map(|input| input.name()).collect();
    for required in [
        "input_ids",
        "attention_mask",
        "token_type_ids",
        "num_logits_to_keep",
    ] {
        if !inputs.contains(required) {
            return Err(MimirError::Init(format!(
                "ONNX graph is missing required input {required:?}"
            )));
        }
    }
    let outputs: HashSet<&str> = session
        .outputs()
        .iter()
        .map(|output| output.name())
        .collect();
    if !outputs.contains("logits") {
        return Err(MimirError::Init(
            "ONNX graph is missing required output `logits`".into(),
        ));
    }
    if cache_specs
        .iter()
        .any(|spec| !outputs.contains(spec.output_name.as_str()))
    {
        return Err(MimirError::Init(
            "ONNX graph cache inputs and outputs do not match".into(),
        ));
    }
    Ok(())
}

fn validate_model_file(model_file: &str) -> Result<(), MimirError> {
    let path = Path::new(model_file);
    if path.is_absolute()
        || path
            .components()
            .any(|component| component == Component::ParentDir)
    {
        return Err(MimirError::Init(
            "model_file must be a relative path without `..`".into(),
        ));
    }
    Ok(())
}

fn resolve_model_dir(source: &str, model_file: &str) -> Result<PathBuf, MimirError> {
    let required_files = vec![
        "config.json".to_string(),
        "tokenizer.json".to_string(),
        "tokenizer_config.json".to_string(),
        "chat_template.jinja".to_string(),
        model_file.to_string(),
        format!("{model_file}.data"),
    ];
    Ok(crate::huggingface::download_onnx(
        source,
        &required_files,
        None,
    )?)
}

#[derive(Deserialize)]
struct ModelConfig {
    max_position_embeddings: usize,
    eos_token_id: TokenIds,
}

impl ModelConfig {
    fn from_dir(model_dir: &Path) -> Result<Self, MimirError> {
        let file = std::fs::File::open(model_dir.join("config.json"))?;
        Ok(serde_json::from_reader(file)?)
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TokenIds {
    One(u32),
    Many(Vec<u32>),
}

impl TokenIds {
    fn into_vec(self) -> Vec<u32> {
        match self {
            Self::One(token) => vec![token],
            Self::Many(tokens) => tokens,
        }
    }
}

#[derive(Deserialize)]
struct TokenizerConfig {
    bos_token: String,
    eos_token: String,
}

impl TokenizerConfig {
    fn from_dir(model_dir: &Path) -> Result<Self, MimirError> {
        let file = std::fs::File::open(model_dir.join("tokenizer_config.json"))?;
        Ok(serde_json::from_reader(file)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_cache_names_numerically() {
        let mut names = [
            "past_key_values.10.value",
            "past_key_values.2.value",
            "past_key_values.10.key",
            "past_key_values.2.key",
        ];
        names.sort_by_key(|name| cache_sort_key(name));
        assert_eq!(
            names,
            [
                "past_key_values.2.key",
                "past_key_values.2.value",
                "past_key_values.10.key",
                "past_key_values.10.value",
            ]
        );
    }

    #[test]
    fn rejects_parent_model_path() {
        assert!(validate_model_file("../model.onnx").is_err());
        assert!(validate_model_file("onnx/model.onnx").is_ok());
    }
}
