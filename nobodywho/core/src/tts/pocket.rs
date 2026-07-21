use crate::errors::TtsError;
use crate::tts::architecture::TtsArchitectureImpl;
use crate::tts::TtsDevice;
use ndarray::{Array, ArrayD, IxDyn};
use ort::session::{Session, SessionInputValue, SessionOutputs};
use ort::value::{DynTensor, DynValue, Tensor};
use rand_distr::{Distribution, Normal};
use safetensors::tensor::{Dtype, SafeTensors};
use sentencepiece_rs::SentencePieceProcessor;
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const VOICE_REPOSITORY: &str = "hf://kyutai/pocket-tts";
const DEFAULT_LANGUAGE: &str = "english_2026-04";
const DEFAULT_VOICE: &str = "alba";
const DEFAULT_TEMPERATURE: f32 = 0.7;
const DEFAULT_EOS_THRESHOLD: f32 = -4.0; // Speech ends if the logit is above this.

pub(in crate::tts) struct PocketTtsBackend {
    tokenizer: SentencePieceProcessor,
    text_conditioner: Session,
    flow_lm_main: Session,
    flow_lm_flow: Session,
    mimi_decoder: Session,
    flow_state_manifest: Vec<StateEntry>,
    mimi_state_manifest: Vec<StateEntry>,
    voice_state: State,
    latent_dim: usize,
    conditioning_dim: usize,
    sample_rate: u32,
    frame_rate: f32,
    max_token_per_chunk: usize,
    temperature: f32,
    lsd_steps: usize,
    frames_after_eos: Option<usize>,
}

impl PocketTtsBackend {
    pub fn new(config: &PocketTtsConfig, device: TtsDevice) -> Result<Self, TtsError> {
        config.validate()?;
        let files = config.required_files();
        let model_dir = crate::huggingface::download_onnx(&config.source, &files, None)?;
        let bundle_dir = model_dir.join("onnx").join(&config.language);
        let metadata: BundleMetadata = read_json(&bundle_dir.join("bundle.json"))?;
        let precision = config.precision.file_suffix();
        let tokenizer = SentencePieceProcessor::open(bundle_dir.join(&metadata.tokenizer_file))
            .map_err(|error| TtsError::InvalidAsset {
                path: bundle_dir
                    .join(&metadata.tokenizer_file)
                    .display()
                    .to_string(),
                message: error.to_string(),
            })?;
        if !metadata.predefined_voices.contains(&config.voice) {
            return Err(TtsError::PocketMissingVoice {
                voice: config.voice.clone(),
                available: metadata.predefined_voices.join(", "),
            });
        }
        let flow_state_manifest = metadata.flow_lm_state_manifest;
        let huggingface_token = config.huggingface_token()?;
        let voice_path = download_voice_state(
            &config.language,
            &config.voice,
            huggingface_token.as_deref(),
        )?;
        let voice_state = State::from_safetensors(&voice_path, &flow_state_manifest)?;

        Ok(Self {
            tokenizer,
            text_conditioner: crate::onnx::load_session(
                &bundle_dir.join("text_conditioner.onnx"),
                device,
            )?,
            flow_lm_main: crate::onnx::load_session(
                &bundle_dir.join(format!("flow_lm_main{precision}.onnx")),
                device,
            )?,
            flow_lm_flow: crate::onnx::load_session(
                &bundle_dir.join(format!("flow_lm_flow{precision}.onnx")),
                device,
            )?,
            mimi_decoder: crate::onnx::load_session(
                &bundle_dir.join(format!("mimi_decoder{precision}.onnx")),
                device,
            )?,
            flow_state_manifest,
            mimi_state_manifest: metadata.mimi_state_manifest,
            voice_state,
            latent_dim: metadata.latent_dim,
            conditioning_dim: metadata.conditioning_dim,
            sample_rate: metadata.sample_rate,
            frame_rate: metadata.frame_rate,
            max_token_per_chunk: metadata.max_token_per_chunk,
            temperature: config.temperature,
            lsd_steps: config.lsd_steps,
            frames_after_eos: metadata.model_recommended_frames_after_eos,
        })
    }

    fn synthesize(&mut self, text: &str) -> Result<Vec<f32>, TtsError> {
        let chunks = self.chunk_text(text)?;
        let mut audio = Vec::new();
        for chunk in chunks {
            let token_ids = self.tokenize(&chunk)?;
            let latents = self.generate_latents(token_ids)?;
            audio.extend(self.decode_latents(latents)?);
        }
        Ok(audio)
    }

    fn chunk_text(&self, text: &str) -> Result<Vec<String>, TtsError> {
        let text = prepare_text(text)?;
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut chunks = Vec::new();
        let mut current = String::new();
        for word in words {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            let len = self
                .tokenizer
                .encode_to_ids(&candidate)
                .map_err(|error| TtsError::InvalidConfig {
                    message: format!("Pocket TTS tokenization failed: {error}"),
                })?
                .len();
            if !current.is_empty() && len > self.max_token_per_chunk {
                chunks.push(current);
                current = word.to_string();
            } else {
                current = candidate;
            }
        }
        if !current.is_empty() {
            chunks.push(current);
        }
        Ok(chunks)
    }

    fn tokenize(&self, text: &str) -> Result<DynTensor, TtsError> {
        let token_ids =
            self.tokenizer
                .encode_to_ids(text)
                .map_err(|error| TtsError::InvalidConfig {
                    message: format!("Pocket TTS tokenization failed: {error}"),
                })?;
        let token_ids: Vec<i64> = token_ids.into_iter().map(|id| id as i64).collect();
        Ok(Tensor::from_array(Array::from_shape_vec((1, token_ids.len()), token_ids)?)?.upcast())
    }

    fn generate_latents(&mut self, token_ids: DynTensor) -> Result<Vec<DynTensor>, TtsError> {
        let token_count = token_ids.shape().iter().product::<i64>();
        let text_embeddings = {
            let outputs = self
                .text_conditioner
                .run(ort::inputs! { "token_ids" => token_ids })?;
            tensor_from_output_f32(&outputs["embeddings"])?
        };
        let mut state = self.voice_state.clone();
        let empty_sequence = tensor_f32(&[1, 0, self.latent_dim], 0.0)?;
        self.run_flow_main(&empty_sequence, &text_embeddings, &mut state)?;
        let empty_text = tensor_f32(&[1, 0, self.conditioning_dim], 0.0)?;
        let mut current = tensor_f32(&[1, 1, self.latent_dim], f32::NAN)?;
        let frame_limit = ((token_count as f32 / 3.0 + 2.0) * self.frame_rate).ceil() as usize;
        let mut eos_step = None;
        let mut latents = Vec::new();

        for step in 0..frame_limit {
            let (conditioning, eos) = self.run_flow_main(&current, &empty_text, &mut state)?;
            if eos > DEFAULT_EOS_THRESHOLD && eos_step.is_none() {
                eos_step = Some(step);
            }
            let after_eos = self.frames_after_eos.unwrap_or(3);
            if eos_step.is_some_and(|first| step >= first + after_eos) {
                break;
            }
            let mut noise = vec![0.0; self.latent_dim];
            if self.temperature > 0.0 {
                let distribution = Normal::new(0.0, self.temperature.sqrt()).map_err(|error| {
                    TtsError::InvalidConfig {
                        message: error.to_string(),
                    }
                })?;
                for value in &mut noise {
                    *value = distribution.sample(&mut rand::rng());
                }
            }
            for step_index in 0..self.lsd_steps {
                let flow_time_start = step_index as f32 / self.lsd_steps as f32;
                let flow_time_end = (step_index + 1) as f32 / self.lsd_steps as f32;
                let flow_outputs = self.flow_lm_flow.run(ort::inputs! {
                    "c" => &conditioning,
                    "s" => tensor_f32(&[1, 1], flow_time_start)?,
                    "t" => tensor_f32(&[1, 1], flow_time_end)?,
                    "x" => Tensor::from_array(Array::from_shape_vec((1, self.latent_dim), noise.clone())?)?,
                })?;
                let flow_direction = flow_outputs[0].try_extract_tensor::<f32>()?.1.to_vec();
                for (value, update) in noise.iter_mut().zip(flow_direction) {
                    *value += update / self.lsd_steps as f32;
                }
            }
            current = Tensor::from_array(Array::from_shape_vec((1, 1, self.latent_dim), noise)?)?
                .upcast();
            latents.push(current.clone());
        }
        Ok(latents)
    }

    fn run_flow_main(
        &mut self,
        sequence: &DynTensor,
        text_embeddings: &DynTensor,
        state: &mut State,
    ) -> Result<(DynTensor, f32), TtsError> {
        let mut inputs = ort::inputs! {
            "sequence" => sequence,
            "text_embeddings" => text_embeddings,
        };
        inputs.extend(state.inputs()?);
        let outputs = self.flow_lm_main.run(inputs)?;
        let conditioning = tensor_from_output_f32(&outputs["conditioning"])?;
        let eos = outputs["eos_logit"].try_extract_tensor::<f32>()?.1[0];
        state.update(&outputs, &self.flow_state_manifest)?;
        Ok((conditioning, eos))
    }

    fn decode_latents(&mut self, latents: Vec<DynTensor>) -> Result<Vec<f32>, TtsError> {
        let mut state = <State as StateExt>::new(&self.mimi_state_manifest)?;
        let mut audio = Vec::new();
        for chunk in latents.chunks(12) {
            let mut values = Vec::with_capacity(chunk.len() * self.latent_dim);
            for latent in chunk {
                values.extend_from_slice(latent.try_extract_tensor::<f32>()?.1);
            }
            let latent = Tensor::from_array(Array::from_shape_vec(
                (1, chunk.len(), self.latent_dim),
                values,
            )?)?
            .upcast();
            let mut inputs = ort::inputs! { "latent" => latent };
            inputs.extend(state.inputs()?);
            let outputs = self.mimi_decoder.run(inputs)?;
            audio.extend_from_slice(outputs["audio_frame"].try_extract_tensor::<f32>()?.1);
            state.update(&outputs, &self.mimi_state_manifest)?;
        }
        Ok(audio)
    }
}

impl TtsArchitectureImpl for PocketTtsBackend {
    fn synthesize_raw(&mut self, text: &str) -> Result<Vec<f32>, TtsError> {
        self.synthesize(text)
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

#[derive(Clone, Debug)]
pub struct PocketTtsConfig {
    pub source: String,
    pub language: String,
    pub voice: String,
    pub precision: PocketTtsPrecision,
    pub temperature: f32,
    pub lsd_steps: usize,
    /// Hugging Face access token for Kyutai's gated built-in voice states.
    pub huggingface_token: Option<String>,
}

impl PocketTtsConfig {
    pub fn new(source: impl AsRef<str>) -> Self {
        Self {
            source: source.as_ref().to_string(),
            language: DEFAULT_LANGUAGE.into(),
            voice: DEFAULT_VOICE.into(),
            precision: PocketTtsPrecision::Int8,
            temperature: DEFAULT_TEMPERATURE,
            lsd_steps: 1,
            huggingface_token: None,
        }
    }

    fn huggingface_token(&self) -> Result<Option<String>, TtsError> {
        if let Some(token) = &self.huggingface_token {
            return Ok(Some(token.clone()));
        }
        match std::env::var("HF_TOKEN") {
            Ok(token) => Ok(Some(token)),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(std::env::VarError::NotUnicode(_)) => Err(TtsError::InvalidConfig {
                message: "HF_TOKEN must be valid Unicode".into(),
            }),
        }
    }

    fn validate(&self) -> Result<(), TtsError> {
        if !self.temperature.is_finite() || self.temperature < 0.0 {
            return Err(TtsError::InvalidConfig {
                message: "Pocket TTS temperature must be finite and non-negative".into(),
            });
        }
        if self.lsd_steps == 0 {
            return Err(TtsError::InvalidConfig {
                message: "Pocket TTS lsd_steps must be greater than 0".into(),
            });
        }
        Ok(())
    }

    fn required_files(&self) -> Vec<String> {
        let suffix = self.precision.file_suffix();
        let base = format!("onnx/{}", self.language);
        [
            "bundle.json".to_string(),
            "tokenizer.model".to_string(),
            "text_conditioner.onnx".to_string(),
            format!("flow_lm_main{suffix}.onnx"),
            format!("flow_lm_flow{suffix}.onnx"),
            format!("mimi_decoder{suffix}.onnx"),
        ]
        .into_iter()
        .map(|file| format!("{base}/{file}"))
        .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PocketTtsPrecision {
    Int8,
    Fp32,
}

impl PocketTtsPrecision {
    fn file_suffix(self) -> &'static str {
        match self {
            Self::Int8 => "_int8",
            Self::Fp32 => "",
        }
    }
}

#[derive(Deserialize)]
struct BundleMetadata {
    tokenizer_file: String,
    flow_lm_state_manifest: Vec<StateEntry>,
    mimi_state_manifest: Vec<StateEntry>,
    latent_dim: usize,
    conditioning_dim: usize,
    sample_rate: u32,
    frame_rate: f32,
    max_token_per_chunk: usize,
    model_recommended_frames_after_eos: Option<usize>,
    predefined_voices: Vec<String>,
}

#[derive(Clone, Deserialize)]
struct StateEntry {
    input_name: String,
    output_name: String,
    module: String,
    key: String,
    dtype: String,
    shape: Vec<usize>,
    fill: String,
}

#[derive(Clone)]
enum StateValue {
    Float(Vec<f32>, Vec<usize>),
    Int(Vec<i64>, Vec<usize>),
    Bool(Vec<bool>, Vec<usize>),
}

type State = HashMap<String, StateValue>;

trait StateExt {
    fn new(manifest: &[StateEntry]) -> Result<Self, TtsError>
    where
        Self: Sized;
    fn from_safetensors(path: &Path, manifest: &[StateEntry]) -> Result<Self, TtsError>
    where
        Self: Sized;
    fn inputs(&self) -> Result<Vec<(Cow<'static, str>, SessionInputValue<'static>)>, TtsError>;
    fn update(
        &mut self,
        outputs: &SessionOutputs<'_>,
        manifest: &[StateEntry],
    ) -> Result<(), TtsError>;
}

impl StateExt for State {
    fn new(manifest: &[StateEntry]) -> Result<Self, TtsError> {
        Ok(manifest
            .iter()
            .map(|entry| (entry.input_name.clone(), StateValue::filled(entry)))
            .collect())
    }

    fn from_safetensors(path: &Path, manifest: &[StateEntry]) -> Result<Self, TtsError> {
        let bytes = std::fs::read(path)?;
        let tensors = SafeTensors::deserialize(&bytes).map_err(|error| TtsError::InvalidAsset {
            path: path.display().to_string(),
            message: error.to_string(),
        })?;
        let mut state = <State as StateExt>::new(manifest)?;
        for entry in manifest {
            let name = format!("{}/{}", entry.module, entry.key);
            let value = if let Ok(tensor) = tensors.tensor(&name) {
                StateValue::from_tensor(&tensor, entry)?
            } else if entry.key == "step" {
                let offset_name = format!("{}/offset", entry.module);
                tensors
                    .tensor(&offset_name)
                    .ok()
                    .map(|tensor| StateValue::from_tensor(&tensor, entry))
                    .transpose()?
                    .unwrap_or_else(|| StateValue::filled(entry))
            } else {
                continue;
            };
            state.insert(entry.input_name.clone(), value);
        }
        Ok(state)
    }

    fn inputs(&self) -> Result<Vec<(Cow<'static, str>, SessionInputValue<'static>)>, TtsError> {
        self.iter()
            .map(|(name, value)| {
                Ok((
                    Cow::Owned(name.clone()),
                    SessionInputValue::Owned(value.tensor()?.into()),
                ))
            })
            .collect()
    }

    fn update(
        &mut self,
        outputs: &SessionOutputs<'_>,
        manifest: &[StateEntry],
    ) -> Result<(), TtsError> {
        for entry in manifest {
            let output = &outputs[entry.output_name.as_str()];
            self.insert(
                entry.input_name.clone(),
                StateValue::from_output(output, entry)?,
            );
        }
        Ok(())
    }
}

impl StateValue {
    fn filled(entry: &StateEntry) -> Self {
        let len = entry.shape.iter().product();
        match entry.dtype.as_str() {
            "float32" => Self::Float(
                vec![if entry.fill == "nan" { f32::NAN } else { 0.0 }; len],
                entry.shape.clone(),
            ),
            "int64" => Self::Int(vec![0; len], entry.shape.clone()),
            "bool" => Self::Bool(vec![entry.fill == "ones"; len], entry.shape.clone()),
            _ => unreachable!("unsupported Pocket TTS state dtype"),
        }
    }

    fn from_tensor(
        tensor: &safetensors::tensor::TensorView<'_>,
        entry: &StateEntry,
    ) -> Result<Self, TtsError> {
        let mut value = Self::filled(entry);
        match (&mut value, tensor.dtype()) {
            (Self::Float(data, shape), Dtype::F32) => {
                copy_bytes_f32(data, shape, tensor.data(), tensor.shape())
            }
            (Self::Int(data, shape), Dtype::I64) => {
                copy_bytes_i64(data, shape, tensor.data(), tensor.shape())
            }
            _ => {
                return Err(TtsError::InvalidAsset {
                    path: format!("Pocket TTS voice state: {}/{}", entry.module, entry.key),
                    message: format!("expected {}, found {:?}", entry.dtype, tensor.dtype()),
                });
            }
        }
        Ok(value)
    }

    fn from_output(tensor: &DynValue, entry: &StateEntry) -> Result<Self, TtsError> {
        match entry.dtype.as_str() {
            "float32" => {
                let (shape, data) = tensor.try_extract_tensor::<f32>()?;
                Ok(Self::Float(
                    data.to_vec(),
                    shape.iter().map(|&value| value as usize).collect(),
                ))
            }
            "int64" => {
                let (shape, data) = tensor.try_extract_tensor::<i64>()?;
                Ok(Self::Int(
                    data.to_vec(),
                    shape.iter().map(|&value| value as usize).collect(),
                ))
            }
            "bool" => {
                let (shape, data) = tensor.try_extract_tensor::<bool>()?;
                Ok(Self::Bool(
                    data.to_vec(),
                    shape.iter().map(|&value| value as usize).collect(),
                ))
            }
            _ => Err(TtsError::InvalidAsset {
                path: "Pocket TTS state".into(),
                message: format!("unsupported dtype {}", entry.dtype),
            }),
        }
    }

    fn tensor(&self) -> Result<DynTensor, TtsError> {
        match self {
            Self::Float(data, shape) => Ok(Tensor::from_array(ArrayD::from_shape_vec(
                IxDyn(shape),
                data.clone(),
            )?)?
            .upcast()),
            Self::Int(data, shape) => Ok(Tensor::from_array(ArrayD::from_shape_vec(
                IxDyn(shape),
                data.clone(),
            )?)?
            .upcast()),
            Self::Bool(data, shape) => Ok(Tensor::from_array(ArrayD::from_shape_vec(
                IxDyn(shape),
                data.clone(),
            )?)?
            .upcast()),
        }
    }
}

fn download_voice_state(
    language: &str,
    voice: &str,
    token: Option<&str>,
) -> Result<PathBuf, TtsError> {
    let filename = format!("languages/{language}/embeddings/{voice}.safetensors");
    let directory = crate::huggingface::download_onnx(
        VOICE_REPOSITORY,
        std::slice::from_ref(&filename),
        token,
    )?;
    Ok(directory.join(filename))
}

fn tensor_f32(shape: &[usize], value: f32) -> Result<DynTensor, TtsError> {
    Ok(Tensor::from_array(ArrayD::from_elem(IxDyn(shape), value))?.upcast())
}

fn tensor_from_output_f32(value: &DynValue) -> Result<DynTensor, TtsError> {
    let (shape, data) = value.try_extract_tensor::<f32>()?;
    Ok(Tensor::from_array(ArrayD::from_shape_vec(
        IxDyn(
            &shape
                .iter()
                .map(|&value| value as usize)
                .collect::<Vec<_>>(),
        ),
        data.to_vec(),
    )?)?
    .upcast())
}

fn prepare_text(text: &str) -> Result<String, TtsError> {
    let mut text = text.trim().replace(['\n', '\r'], " ");
    if text.is_empty() {
        return Err(TtsError::EmptyText);
    }
    while text.contains("  ") {
        text = text.replace("  ", " ");
    }
    if let Some(first) = text.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    if text.ends_with(char::is_alphanumeric) {
        text.push('.');
    }
    Ok(text)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, TtsError> {
    Ok(serde_json::from_reader(std::fs::File::open(path)?)?)
}

fn copy_bytes_f32(
    target: &mut [f32],
    target_shape: &[usize],
    source: &[u8],
    source_shape: &[usize],
) {
    let source: Vec<f32> = source
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes(bytes.try_into().expect("f32 chunks")))
        .collect();
    copy_tensor_values(target, target_shape, &source, source_shape);
}

fn copy_bytes_i64(
    target: &mut [i64],
    target_shape: &[usize],
    source: &[u8],
    source_shape: &[usize],
) {
    let source: Vec<i64> = source
        .chunks_exact(8)
        .map(|bytes| i64::from_le_bytes(bytes.try_into().expect("i64 chunks")))
        .collect();
    copy_tensor_values(target, target_shape, &source, source_shape);
}

fn copy_tensor_values<T: Copy>(
    target: &mut [T],
    target_shape: &[usize],
    source: &[T],
    source_shape: &[usize],
) {
    if target_shape.len() != source_shape.len() {
        return;
    }
    for (source_index, value) in source.iter().enumerate() {
        let mut remainder = source_index;
        let mut target_index = 0;
        for dimension in 0..source_shape.len() {
            let stride = source_shape[dimension + 1..].iter().product::<usize>();
            let coordinate = remainder / stride;
            remainder %= stride;
            if coordinate >= target_shape[dimension] {
                target_index = target.len();
                break;
            }
            target_index = target_index * target_shape[dimension] + coordinate;
        }
        if let Some(output) = target.get_mut(target_index) {
            *output = *value;
        }
    }
}
