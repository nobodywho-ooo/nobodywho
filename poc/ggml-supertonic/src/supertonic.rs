use anyhow::{bail, Context, Result};
use ggml_runtime::{Backend, BackendKind, GraphBuilder, Shape, Tensor, Weights};
use rand::SeedableRng;
use rand_distr::{Distribution, StandardNormal};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;
use std::time::Instant;

const DURATION_METADATA_BYTES: usize = 256 * 1024 * 1024;
const TEXT_METADATA_BYTES: usize = 1024 * 1024 * 1024;
const VECTOR_METADATA_BYTES: usize = 512 * 1024 * 1024;
const VOCODER_METADATA_BYTES: usize = 2048 * 1024 * 1024;
const MAX_DURATION_SECONDS: f32 = 60.0;

pub struct EngineConfig {
    pub model_dir: PathBuf,
    pub backend: String,
    pub voice: String,
    pub language: String,
    pub steps: usize,
    pub speed: f32,
    pub seed: u64,
    pub threads: usize,
    pub debug_dir: Option<PathBuf>,
}

pub struct Audio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

#[derive(Deserialize)]
struct TtsConfig {
    ae: AutoencoderConfig,
    ttl: TextLatentConfig,
}

#[derive(Deserialize)]
struct AutoencoderConfig {
    sample_rate: i64,
    base_chunk_size: i64,
}

#[derive(Deserialize)]
struct TextLatentConfig {
    chunk_compress_factor: i64,
    latent_dim: i64,
}

#[derive(Deserialize)]
struct StyleFile {
    style_ttl: StyleComponent,
    style_dp: StyleComponent,
}

#[derive(Deserialize)]
struct StyleComponent {
    data: Vec<Vec<Vec<f32>>>,
    dims: Vec<i64>,
}

impl StyleComponent {
    fn flatten(self) -> Vec<f32> {
        self.data.into_iter().flatten().flatten().collect()
    }
}

pub struct Engine {
    backend: Backend,
    weights: Weights,
    config: TtsConfig,
    unicode_indexer: Vec<i32>,
    style_ttl: Vec<f32>,
    style_ttl_shape: Vec<i64>,
    style_dp: Vec<f32>,
    style_dp_shape: Vec<i64>,
    steps: usize,
    speed: f32,
    seed: u64,
    threads: usize,
    debug_dir: Option<PathBuf>,
}

#[derive(Serialize)]
struct DebugTensor<'a, T> {
    shape: &'a [usize],
    data: &'a [T],
}

impl Engine {
    pub fn new(config: EngineConfig) -> Result<Self> {
        if config.steps == 0 || !config.speed.is_finite() || config.speed <= 0.0 {
            bail!("steps and speed must be positive");
        }
        if !matches!(
            config.language.as_str(),
            "en" | "ko"
                | "ja"
                | "ar"
                | "bg"
                | "cs"
                | "da"
                | "de"
                | "el"
                | "es"
                | "et"
                | "fi"
                | "fr"
                | "hi"
                | "hr"
                | "hu"
                | "id"
                | "it"
                | "lt"
                | "lv"
                | "nl"
                | "pl"
                | "pt"
                | "ro"
                | "ru"
                | "sk"
                | "sl"
                | "sv"
                | "tr"
                | "uk"
                | "vi"
                | "na"
        ) {
            bail!("unsupported Supertonic language {}", config.language);
        }
        let backend_kind = match config.backend.to_ascii_lowercase().as_str() {
            "cpu" => BackendKind::Cpu,
            "metal" => BackendKind::Metal,
            value => bail!("unsupported backend {value}; choose cpu or metal"),
        };
        let backend = Backend::new(backend_kind)?;
        backend.set_threads(config.threads);
        let model_path = config.model_dir.join("supertonic-3-orig.gguf");
        let started = Instant::now();
        let weights = Weights::load(&model_path, &backend)?;
        let tts_config: TtsConfig = read_json(&config.model_dir.join("tts.json"))?;
        let unicode_indexer_i64: Vec<i64> =
            read_json(&config.model_dir.join("unicode_indexer.json"))?;
        let unicode_indexer = unicode_indexer_i64
            .into_iter()
            .map(|value| i32::try_from(value).context("unicode index exceeds i32"))
            .collect::<Result<Vec<_>>>()?;
        let style: StyleFile = read_json(
            &config
                .model_dir
                .join("voice_styles")
                .join(format!("{}.json", config.voice)),
        )?;
        let style_ttl_shape = style.style_ttl.dims.clone();
        let style_dp_shape = style.style_dp.dims.clone();
        let style_ttl = style.style_ttl.flatten();
        let style_dp = style.style_dp.flatten();
        println!(
            "Loaded {} Supertonic tensors on {} in {:.2}s",
            weights.len(),
            backend_kind.name(),
            started.elapsed().as_secs_f32()
        );
        Ok(Self {
            backend,
            weights,
            config: tts_config,
            unicode_indexer,
            style_ttl,
            style_ttl_shape,
            style_dp,
            style_dp_shape,
            steps: config.steps,
            speed: config.speed,
            seed: config.seed,
            threads: config.threads,
            debug_dir: config.debug_dir,
        })
    }

    pub fn synthesize(&self, processed_text: String) -> Result<Audio> {
        let ids: Vec<i32> = processed_text
            .chars()
            .map(|character| {
                self.unicode_indexer
                    .get(character as usize)
                    .copied()
                    .filter(|index| *index >= 0)
                    .with_context(|| {
                        format!(
                            "Supertonic unicode indexer has no entry for U+{:04X}",
                            character as u32
                        )
                    })
            })
            .collect::<Result<_>>()?;
        if ids.is_empty() {
            bail!("text is empty");
        }
        let mask = vec![1.0; ids.len()];
        self.dump("text_ids", &[1, ids.len()], &ids)?;
        self.dump("text_mask", &[1, 1, ids.len()], &mask)?;
        let started = Instant::now();
        let predicted_duration = self.predict_duration(&ids, &mask)?;
        self.dump("predicted_duration", &[1], &[predicted_duration])?;
        let duration = predicted_duration / self.speed;
        if !duration.is_finite() || duration <= 0.0 || duration > MAX_DURATION_SECONDS {
            bail!("duration predictor returned unsupported duration {duration}");
        }
        let text_embedding = self.encode_text(&ids, &mask)?;
        self.dump("text_embedding", &[1, 256, ids.len()], &text_embedding)?;
        let chunk_size = self.config.ae.base_chunk_size * self.config.ttl.chunk_compress_factor;
        let sample_count = (duration * self.config.ae.sample_rate as f32) as usize;
        let latent_length = (sample_count as i64 + chunk_size - 1) / chunk_size;
        let latent_channels = self.config.ttl.latent_dim * self.config.ttl.chunk_compress_factor;
        let mut rng = rand::rngs::StdRng::seed_from_u64(self.seed);
        let normal = StandardNormal;
        let mut latent: Vec<f32> = (0..latent_channels * latent_length)
            .map(|_| normal.sample(&mut rng))
            .collect();
        self.dump(
            "latent_initial",
            &[1, latent_channels as usize, latent_length as usize],
            &latent,
        )?;
        latent = self.denoise(
            latent,
            latent_length,
            &text_embedding,
            ids.len() as i64,
            &mask,
        )?;
        self.dump(
            "latent_final",
            &[1, latent_channels as usize, latent_length as usize],
            &latent,
        )?;
        let mut samples = self.vocode(&latent, latent_length)?;
        self.dump("vocoder_output", &[1, samples.len()], &samples)?;
        samples.truncate(sample_count.min(samples.len()));
        println!(
            "Synthesized {:.2}s of audio in {:.2}s with {} threads",
            duration,
            started.elapsed().as_secs_f32(),
            self.threads
        );
        Ok(Audio {
            samples,
            sample_rate: self.config.ae.sample_rate as u32,
        })
    }

    fn dump<T: Serialize>(&self, name: &str, shape: &[usize], data: &[T]) -> Result<()> {
        let Some(directory) = &self.debug_dir else {
            return Ok(());
        };
        std::fs::create_dir_all(directory)?;
        let file = File::create(directory.join(format!("{name}.json")))?;
        serde_json::to_writer(BufWriter::new(file), &DebugTensor { shape, data })?;
        Ok(())
    }

    fn predict_duration(&self, ids: &[i32], mask: &[f32]) -> Result<f32> {
        let graph_builder = GraphBuilder::new(DURATION_METADATA_BYTES)?;
        let ids_tensor = graph_builder.input_i32(&[1, ids.len() as i64])?;
        let mask_tensor = graph_builder.input_f32(&[1, 1, ids.len() as i64])?;
        let style_tensor = graph_builder.input_f32(&self.style_dp_shape)?;
        let network = Network::new(&graph_builder, &self.weights);
        let output =
            graph_builder.contiguous(network.duration(ids_tensor, mask_tensor, style_tensor)?)?;
        let graph = graph_builder.finish(output, &self.backend)?;
        graph.set_i32(ids_tensor, ids)?;
        graph.set_f32(mask_tensor, mask)?;
        graph.set_f32(style_tensor, &self.style_dp)?;
        graph.compute(&self.backend)?;
        Ok(graph.output_f32()?[0])
    }

    fn encode_text(&self, ids: &[i32], mask: &[f32]) -> Result<Vec<f32>> {
        let graph_builder = GraphBuilder::new(TEXT_METADATA_BYTES)?;
        let ids_tensor = graph_builder.input_i32(&[1, ids.len() as i64])?;
        let mask_tensor = graph_builder.input_f32(&[1, 1, ids.len() as i64])?;
        let style_tensor = graph_builder.input_f32(&self.style_ttl_shape)?;
        let network = Network::new(&graph_builder, &self.weights);
        let output = graph_builder.contiguous(network.text_encoder(
            ids_tensor,
            mask_tensor,
            style_tensor,
        )?)?;
        let graph = graph_builder.finish(output, &self.backend)?;
        graph.set_i32(ids_tensor, ids)?;
        graph.set_f32(mask_tensor, mask)?;
        graph.set_f32(style_tensor, &self.style_ttl)?;
        graph.compute(&self.backend)?;
        graph.output_f32()
    }

    fn denoise(
        &self,
        mut latent: Vec<f32>,
        latent_length: i64,
        text_embedding: &[f32],
        text_length: i64,
        text_mask: &[f32],
    ) -> Result<Vec<f32>> {
        let latent_channels = self.config.ttl.latent_dim * self.config.ttl.chunk_compress_factor;
        let latent_mask = vec![1.0; latent_length as usize];
        for step in 0..self.steps {
            let graph_builder = GraphBuilder::new(VECTOR_METADATA_BYTES)?;
            let latent_tensor = graph_builder.input_f32(&[1, latent_channels, latent_length])?;
            let text_tensor = graph_builder.input_f32(&[1, 256, text_length])?;
            let text_mask_tensor = graph_builder.input_f32(&[1, 1, text_length])?;
            let style_tensor = graph_builder.input_f32(&self.style_ttl_shape)?;
            let latent_mask_tensor = graph_builder.input_f32(&[1, 1, latent_length])?;
            let current_step_tensor = graph_builder.input_f32(&[1])?;
            let total_step_tensor = graph_builder.input_f32(&[1])?;
            let network = Network::new(&graph_builder, &self.weights);
            let output = graph_builder.contiguous(network.vector_step(
                latent_tensor,
                text_tensor,
                text_mask_tensor,
                style_tensor,
                latent_mask_tensor,
                current_step_tensor,
                total_step_tensor,
            )?)?;
            let graph = graph_builder.finish(output, &self.backend)?;
            graph.set_f32(latent_tensor, &latent)?;
            graph.set_f32(text_tensor, text_embedding)?;
            graph.set_f32(text_mask_tensor, text_mask)?;
            graph.set_f32(style_tensor, &self.style_ttl)?;
            graph.set_f32(latent_mask_tensor, &latent_mask)?;
            graph.set_f32(current_step_tensor, &[step as f32])?;
            graph.set_f32(total_step_tensor, &[self.steps as f32])?;
            graph.compute(&self.backend)?;
            latent = graph.output_f32()?;
            self.dump(
                &format!("latent_step_{step}"),
                &[1, latent_channels as usize, latent_length as usize],
                &latent,
            )?;
        }
        Ok(latent)
    }

    fn vocode(&self, latent: &[f32], latent_length: i64) -> Result<Vec<f32>> {
        let latent_channels = self.config.ttl.latent_dim * self.config.ttl.chunk_compress_factor;
        let graph_builder = GraphBuilder::new(VOCODER_METADATA_BYTES)?;
        let latent_tensor = graph_builder.input_f32(&[1, latent_channels, latent_length])?;
        let network = Network::new(&graph_builder, &self.weights);
        let output = graph_builder.contiguous(network.vocoder(
            latent_tensor,
            self.config.ttl.latent_dim,
            self.config.ttl.chunk_compress_factor,
            self.config.ae.base_chunk_size,
        )?)?;
        let graph = graph_builder.finish(output, &self.backend)?;
        graph.set_f32(latent_tensor, latent)?;
        graph.compute(&self.backend)?;
        graph.output_f32()
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &PathBuf) -> Result<T> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    serde_json::from_reader(BufReader::new(file))
        .with_context(|| format!("failed to parse {}", path.display()))
}

struct Network<'a> {
    graph: &'a GraphBuilder,
    weights: &'a Weights,
}

impl<'a> Network<'a> {
    fn new(graph: &'a GraphBuilder, weights: &'a Weights) -> Self {
        Self { graph, weights }
    }

    fn weight(&self, name: &str) -> Result<Tensor> {
        self.weights.get(name)
    }

    fn weight_pair(&self, prefix: &str) -> Result<(Tensor, Tensor)> {
        Ok((
            self.weight(&format!("{prefix}.weight"))?,
            self.weight(&format!("{prefix}.bias"))?,
        ))
    }

    fn linear(&self, input: Tensor, prefix: &str) -> Result<Tensor> {
        let (weight, bias) = self.weight_pair(prefix)?;
        self.graph.linear(input, weight, Some(bias))
    }

    fn linear_transposed(&self, input: Tensor, prefix: &str) -> Result<Tensor> {
        let (weight, bias) = self.weight_pair(prefix)?;
        let weight = self
            .graph
            .contiguous(self.graph.transpose(weight, &[1, 0])?)?;
        self.graph.linear(input, weight, Some(bias))
    }

    fn conv(&self, input: Tensor, prefix: &str, dilation: i32) -> Result<Tensor> {
        let (weight, bias) = self.weight_pair(prefix)?;
        self.graph.conv1d(input, weight, Some(bias), dilation)
    }

    fn pointwise(&self, input: Tensor, prefix: &str) -> Result<Tensor> {
        let (mut weight, bias) = self.weight_pair(prefix)?;
        if weight.shape.rank() == 3 {
            weight = self
                .graph
                .reshape(weight, &[weight.shape.at(0), weight.shape.at(1)])?;
        }
        let input = self
            .graph
            .contiguous(self.graph.transpose(input, &[0, 2, 1])?)?;
        let output = self.graph.linear(input, weight, Some(bias))?;
        self.graph.transpose(output, &[0, 2, 1])
    }

    fn norm(&self, input: Tensor, prefix: &str) -> Result<Tensor> {
        self.graph.layer_norm(
            input,
            self.weight(&format!("{prefix}.weight"))?,
            self.weight(&format!("{prefix}.bias"))?,
            1e-6,
        )
    }

    fn convnext(
        &self,
        mut input: Tensor,
        prefix: &str,
        dilations: &[i32],
        pointwise_linear: bool,
        mask: Option<Tensor>,
    ) -> Result<Tensor> {
        for (index, dilation) in dilations.iter().enumerate() {
            let block = format!("{prefix}.convnext.{index}");
            let residual = match mask {
                Some(mask) => self.graph.mul(input, mask)?,
                None => input,
            };
            let padded =
                self.graph
                    .edge_pad(residual, 2 * *dilation as i64, 2 * *dilation as i64, 2)?;
            let output = self.graph.depthwise_conv1d(
                padded,
                self.weight(&format!("{block}.dwconv.weight"))?,
                Some(self.weight(&format!("{block}.dwconv.bias"))?),
                *dilation,
            )?;
            let output = match mask {
                Some(mask) => self.graph.mul(output, mask)?,
                None => output,
            };
            let mut output = self.graph.transpose(output, &[0, 2, 1])?;
            output = self.norm(output, &format!("{block}.norm.norm"))?;
            output = self.graph.transpose(output, &[0, 2, 1])?;
            output = if pointwise_linear {
                self.pointwise(output, &format!("{block}.pwconv1"))?
            } else {
                self.conv(output, &format!("{block}.pwconv1"), 1)?
            };
            output = self.graph.gelu(output)?;
            output = if pointwise_linear {
                self.pointwise(output, &format!("{block}.pwconv2"))?
            } else {
                self.conv(output, &format!("{block}.pwconv2"), 1)?
            };
            output = self
                .graph
                .mul(output, self.weight(&format!("{block}.gamma"))?)?;
            input = self.graph.add(residual, output)?;
        }
        Ok(input)
    }

    fn relative_mask(&self, frames: i64, offset: i64) -> Result<Tensor> {
        let mut values = vec![0.0; (frames * frames) as usize];
        for query in 0..frames {
            let key = query + offset;
            if (0..frames).contains(&key) {
                values[(query * frames + key) as usize] = 1.0;
            }
        }
        self.graph.constant_f32(&[1, 1, frames, frames], values)
    }

    fn conv_self_attention(&self, input: Tensor, prefix: &str, heads: i64) -> Result<Tensor> {
        let q = self.graph.transpose(
            self.conv(input, &format!("{prefix}.conv_q"), 1)?,
            &[0, 2, 1],
        )?;
        let k = self.graph.transpose(
            self.conv(input, &format!("{prefix}.conv_k"), 1)?,
            &[0, 2, 1],
        )?;
        let v = self.graph.transpose(
            self.conv(input, &format!("{prefix}.conv_v"), 1)?,
            &[0, 2, 1],
        )?;
        let head_dim = q.shape.last() / heads;
        let split = |value: Tensor| -> Result<Tensor> {
            let value = self.graph.contiguous(value)?;
            self.graph.transpose(
                self.graph.reshape(
                    value,
                    &[value.shape.at(0), value.shape.at(1), heads, head_dim],
                )?,
                &[0, 2, 1, 3],
            )
        };
        let q = split(q)?;
        let k = split(k)?;
        let v = split(v)?;
        let q_scaled = self.graph.scale(q, 1.0 / (head_dim as f32).sqrt())?;
        let mut scores = self
            .graph
            .matmul(q_scaled, self.graph.transpose(k, &[0, 1, 3, 2])?)?;
        let mut relative_scores: Option<Tensor> = None;
        for offset in -4..=4 {
            let relative = self.graph.slice(
                self.weight(&format!("{prefix}.emb_rel_k"))?,
                1,
                offset + 4,
                1,
            )?;
            let relative = self
                .graph
                .reshape(self.graph.contiguous(relative)?, &[1, 1, 1, head_dim])?;
            let logits = self
                .graph
                .reduce_sum(self.graph.mul(q_scaled, relative)?, 3)?;
            let logits = self.graph.mul(
                self.graph.broadcast(logits, scores.shape)?,
                self.relative_mask(scores.shape.at(2), offset)?,
            )?;
            relative_scores = Some(match relative_scores {
                Some(total) => self.graph.add(total, logits)?,
                None => logits,
            });
        }
        scores = self.graph.add(
            scores,
            relative_scores.context("relative key scores are empty")?,
        )?;
        let attention = self.graph.softmax(self.graph.contiguous(scores)?)?;
        let mut context = self.graph.matmul(attention, v)?;
        let context_shape = Shape::new(&[
            attention.shape.at(0),
            attention.shape.at(1),
            attention.shape.at(2),
            head_dim,
        ])?;
        let mut relative_context: Option<Tensor> = None;
        for offset in -4..=4 {
            let diagonal = self.graph.mul(
                attention,
                self.relative_mask(attention.shape.at(2), offset)?,
            )?;
            let weight_sum = self.graph.reduce_sum(diagonal, 3)?;
            let relative = self.graph.slice(
                self.weight(&format!("{prefix}.emb_rel_v"))?,
                1,
                offset + 4,
                1,
            )?;
            let relative = self
                .graph
                .reshape(self.graph.contiguous(relative)?, &[1, 1, 1, head_dim])?;
            let contribution = self
                .graph
                .broadcast(self.graph.mul(weight_sum, relative)?, context_shape)?;
            relative_context = Some(match relative_context {
                Some(total) => self.graph.add(total, contribution)?,
                None => contribution,
            });
        }
        context = self.graph.add(
            context,
            relative_context.context("relative value context is empty")?,
        )?;
        context = self.graph.transpose(context, &[0, 2, 1, 3])?;
        context = self.graph.reshape(
            self.graph.contiguous(context)?,
            &[input.shape.at(0), input.shape.at(2), input.shape.at(1)],
        )?;
        context = self.graph.transpose(context, &[0, 2, 1])?;
        self.conv(context, &format!("{prefix}.conv_o"), 1)
    }

    fn self_encoder(
        &self,
        mut input: Tensor,
        mask: Tensor,
        prefix: &str,
        layers: i64,
        heads: i64,
    ) -> Result<Tensor> {
        for layer in 0..layers {
            input = self.graph.mul(input, mask)?;
            input = self.graph.add(
                input,
                self.conv_self_attention(input, &format!("{prefix}.attn_layers.{layer}"), heads)?,
            )?;
            let normalized = self.norm(
                self.graph.transpose(input, &[0, 2, 1])?,
                &format!("{prefix}.norm_layers_1.{layer}.norm"),
            )?;
            input = self.graph.transpose(normalized, &[0, 2, 1])?;
            let mut output = self.graph.mul(input, mask)?;
            output = self.conv(output, &format!("{prefix}.ffn_layers.{layer}.conv_1"), 1)?;
            output = self.graph.relu(output)?;
            output = self.graph.mul(output, mask)?;
            output = self.conv(output, &format!("{prefix}.ffn_layers.{layer}.conv_2"), 1)?;
            output = self.graph.mul(output, mask)?;
            input = self.norm(
                self.graph
                    .transpose(self.graph.add(input, output)?, &[0, 2, 1])?,
                &format!("{prefix}.norm_layers_2.{layer}.norm"),
            )?;
            input = self.graph.transpose(input, &[0, 2, 1])?;
        }
        Ok(input)
    }

    fn cross_attention(
        &self,
        query_bct: Tensor,
        key_bct: Tensor,
        value_bct: Tensor,
        prefix: &str,
        heads: i64,
        output_mask: Option<Tensor>,
        tanh_key: bool,
        rotary: bool,
        memory_mask: Option<Tensor>,
    ) -> Result<Tensor> {
        let query = self.graph.transpose(query_bct, &[0, 2, 1])?;
        let key_memory = self.graph.transpose(key_bct, &[0, 2, 1])?;
        let value_memory = self.graph.transpose(value_bct, &[0, 2, 1])?;
        let mut q = self.linear_transposed(query, &format!("{prefix}.W_query.linear"))?;
        let mut k = self.linear_transposed(key_memory, &format!("{prefix}.W_key.linear"))?;
        let mut v = self.linear_transposed(value_memory, &format!("{prefix}.W_value.linear"))?;
        let head_dim = q.shape.last() / heads;
        let split = |value: Tensor| -> Result<Tensor> {
            let value = self.graph.reshape(
                self.graph.contiguous(value)?,
                &[value.shape.at(0), value.shape.at(1), heads, head_dim],
            )?;
            let value = self.graph.transpose(value, &[0, 2, 1, 3])?;
            self.graph.transpose(value, &[1, 0, 2, 3])
        };
        q = split(q)?;
        k = split(k)?;
        v = split(v)?;
        if rotary {
            q = self.rotary64(q, output_mask.context("rotary query mask is missing")?)?;
            k = self.rotary64(k, memory_mask.context("rotary memory mask is missing")?)?;
        }
        if tanh_key {
            k = self.graph.tanh(self.graph.contiguous(k)?)?;
        }
        let mut scores = self
            .graph
            .matmul(q, self.graph.transpose(k, &[0, 1, 3, 2])?)?;
        scores = self.graph.scale(scores, 1.0 / 16.0)?;
        let mut key_mask = None;
        if let Some(memory_mask) = memory_mask {
            let mask = self.graph.reshape(
                self.graph.contiguous(memory_mask)?,
                &[1, memory_mask.shape.at(0), 1, memory_mask.shape.at(2)],
            )?;
            let score_mask = self.graph.scale_bias(mask, 1.0e30, -1.0e30)?;
            scores = self.graph.add(scores, score_mask)?;
            key_mask = Some(mask);
        }
        let mut attention = self.graph.softmax(self.graph.contiguous(scores)?)?;
        if let Some(key_mask) = key_mask {
            attention = self.graph.mul(attention, key_mask)?;
        }
        let mut context = self.graph.matmul(attention, v)?;
        context = self.graph.transpose(context, &[1, 0, 2, 3])?;
        context = self.graph.transpose(context, &[0, 2, 1, 3])?;
        context = self.graph.reshape(
            self.graph.contiguous(context)?,
            &[query.shape.at(0), query.shape.at(1), heads * head_dim],
        )?;
        context = self.linear_transposed(context, &format!("{prefix}.out_fc.linear"))?;
        let mut output = self.graph.transpose(context, &[0, 2, 1])?;
        if let Some(output_mask) = output_mask {
            output = self.graph.mul(output, output_mask)?;
        }
        Ok(output)
    }

    fn rotary64(&self, value: Tensor, mask: Tensor) -> Result<Tensor> {
        let frames = value.shape.at(2);
        let positions: Vec<f32> = (0..frames).map(|value| value as f32).collect();
        let positions = self.graph.constant_f32(&[1, 1, frames, 1], positions)?;
        let active = self.graph.reduce_sum(mask, 2)?;
        let active = self
            .graph
            .reshape(self.graph.contiguous(active)?, &[1, mask.shape.at(0), 1, 1])?;
        let theta = self.graph.reshape(
            self.graph.contiguous(self.weight(
                "vector_estimator.vector_estimator.tts.ttl.vector_field.main_blocks.3.attn.theta",
            )?)?,
            &[1, 1, 1, 32],
        )?;
        let angles = self.graph.mul(self.graph.div(positions, active)?, theta)?;
        let cosine = self.graph.cos(self.graph.contiguous(angles)?)?;
        let sine = self.graph.sin(self.graph.contiguous(angles)?)?;
        let left = self.graph.slice(value, 3, 0, 32)?;
        let right = self.graph.slice(value, 3, 32, 32)?;
        let rotated_left = self
            .graph
            .sub(self.graph.mul(left, cosine)?, self.graph.mul(right, sine)?)?;
        let rotated_right = self
            .graph
            .add(self.graph.mul(left, sine)?, self.graph.mul(right, cosine)?)?;
        self.graph.concat(rotated_left, rotated_right, 3)
    }

    fn duration(&self, ids: Tensor, text_mask: Tensor, style: Tensor) -> Result<Tensor> {
        let mut input = self.graph.embedding(
            ids,
            self.weight(
                "duration_predictor.tts.dp.sentence_encoder.text_embedder.char_embedder.weight",
            )?,
        )?;
        input = self
            .graph
            .mul(input, self.graph.transpose(text_mask, &[0, 2, 1])?)?;
        let sentence_mask = self.graph.slice(text_mask, 2, 0, 1)?;
        let sentence_token = self.graph.transpose(
            self.weight("duration_predictor.tts.dp.sentence_encoder.sentence_token")?,
            &[0, 2, 1],
        )?;
        input = self
            .graph
            .concat(sentence_token, self.graph.contiguous(input)?, 1)?;
        let encoder_mask = self.graph.concat(sentence_mask, text_mask, 2)?;
        input = self.graph.transpose(input, &[0, 2, 1])?;
        input = self.convnext(
            self.graph.contiguous(input)?,
            "duration_predictor.tts.dp.sentence_encoder.convnext",
            &[1, 1, 1, 1, 1, 1],
            true,
            None,
        )?;
        let residual = input;
        input = self.self_encoder(
            input,
            encoder_mask,
            "duration_predictor.tts.dp.sentence_encoder.attn_encoder",
            2,
            2,
        )?;
        input = self.graph.add(input, residual)?;
        input = self.graph.slice(input, 2, 0, 1)?;
        input = self.graph.conv1d(
            input,
            self.weight("duration_predictor.tts.dp.sentence_encoder.proj_out.net.weight")?,
            None,
            1,
        )?;
        input = self.graph.mul(input, sentence_mask)?;
        input = self
            .graph
            .reshape(self.graph.contiguous(input)?, &[1, input.shape.at(1)])?;
        let style = self.graph.reshape(
            self.graph.contiguous(style)?,
            &[1, style.shape.at(1) * style.shape.at(2)],
        )?;
        input = self.graph.concat(input, style, 1)?;
        input = self.linear(input, "duration_predictor.tts.dp.predictor.layers.0")?;
        input = self.graph.prelu(
            input,
            self.weight("duration_predictor.tts.dp.predictor.activation.weight")?,
        )?;
        input = self.linear(input, "duration_predictor.tts.dp.predictor.layers.1")?;
        self.graph.exp(input)
    }

    fn text_encoder(&self, ids: Tensor, text_mask: Tensor, style: Tensor) -> Result<Tensor> {
        let input = self.graph.embedding(
            ids,
            self.weight("text_encoder.tts.ttl.text_encoder.text_embedder.char_embedder.weight")?,
        )?;
        let mut input = self.graph.transpose(input, &[0, 2, 1])?;
        input = self.convnext(
            self.graph.contiguous(input)?,
            "text_encoder.tts.ttl.text_encoder.convnext",
            &[1, 1, 2, 2, 4, 4],
            true,
            None,
        )?;
        let residual = input;
        input = self.self_encoder(
            input,
            text_mask,
            "text_encoder.tts.ttl.text_encoder.attn_encoder",
            4,
            4,
        )?;
        input = self
            .graph
            .mul(self.graph.add(input, residual)?, text_mask)?;
        let style_value = self
            .graph
            .contiguous(self.graph.transpose(style, &[0, 2, 1])?)?;
        let style_key = self.graph.contiguous(self.graph.transpose(
            self.weight("text_encoder.tts.ttl.style_encoder.style_token_layer.style_key")?,
            &[0, 2, 1],
        )?)?;
        let prompted = input;
        input = self.graph.add(
            prompted,
            self.cross_attention(
                prompted,
                style_key,
                style_value,
                "text_encoder.tts.ttl.speech_prompted_text_encoder.attention1",
                2,
                Some(text_mask),
                true,
                false,
                None,
            )?,
        )?;
        input = self.graph.add(
            prompted,
            self.cross_attention(
                input,
                style_key,
                style_value,
                "text_encoder.tts.ttl.speech_prompted_text_encoder.attention2",
                2,
                Some(text_mask),
                true,
                false,
                None,
            )?,
        )?;
        input = self.norm(
            self.graph.transpose(input, &[0, 2, 1])?,
            "text_encoder.tts.ttl.speech_prompted_text_encoder.norm.norm",
        )?;
        input = self.graph.transpose(input, &[0, 2, 1])?;
        self.graph.mul(input, text_mask)
    }

    fn time_embedding(&self, current: Tensor, total: Tensor) -> Result<Tensor> {
        let frequencies = vec![
            1.0,
            0.7429639,
            0.55199546,
            0.4101127,
            0.30469894,
            0.22638035,
            0.16819243,
            0.12496091,
            0.092841454,
            0.068977855,
            0.051248062,
            0.038075458,
            0.028288694,
            0.021017481,
            0.015615228,
            0.011601552,
            0.008619536,
            0.006404005,
            0.004757944,
            0.003534982,
            0.002626364,
            0.001951293,
            0.001449740,
            0.001077105,
            0.0008002502,
            0.0005945571,
            0.0004417345,
            0.00032819266,
            0.00024383534,
            0.00018116087,
            0.000134596,
            0.0001,
        ];
        let frequencies = self.graph.constant_f32(&[1, 32], frequencies)?;
        let ratio = self.graph.scale(self.graph.div(current, total)?, 1000.0)?;
        let arguments = self.graph.mul(ratio, frequencies)?;
        let encoded =
            self.graph
                .concat(self.graph.sin(arguments)?, self.graph.cos(arguments)?, 1)?;
        let mut output = self.linear(
            encoded,
            "vector_estimator.vector_estimator.tts.ttl.vector_field.time_encoder.mlp.0.linear",
        )?;
        output = self
            .graph
            .mul(output, self.graph.tanh(self.graph.softplus(output)?)?)?;
        self.linear(
            output,
            "vector_estimator.vector_estimator.tts.ttl.vector_field.time_encoder.mlp.2.linear",
        )
    }

    fn add_time(&self, input: Tensor, time: Tensor, block_index: i32) -> Result<Tensor> {
        let prefix = format!("vector_estimator.vector_estimator.tts.ttl.vector_field.main_blocks.{block_index}.linear.linear");
        let condition = self.linear_transposed(time, &prefix)?;
        let condition = self.graph.reshape(
            self.graph.contiguous(condition)?,
            &[1, condition.shape.at(1), 1],
        )?;
        self.graph.add(input, condition)
    }

    fn normalize_vector(&self, input: Tensor, mask: Tensor, block: &str) -> Result<Tensor> {
        let normalized = self.norm(
            self.graph.transpose(input, &[0, 2, 1])?,
            &format!("{block}.norm.norm"),
        )?;
        self.graph
            .mul(self.graph.transpose(normalized, &[0, 2, 1])?, mask)
    }

    fn vector_step(
        &self,
        noisy: Tensor,
        text: Tensor,
        text_mask: Tensor,
        style: Tensor,
        latent_mask: Tensor,
        current_step: Tensor,
        total_step: Tensor,
    ) -> Result<Tensor> {
        let unconditioned_text = self.graph.broadcast(
            self.weight(
                "vector_estimator.vector_estimator.tts.ttl.uncond_masker.text_special_token",
            )?,
            text.shape,
        )?;
        let mask_pair = self.graph.concat(latent_mask, latent_mask, 0)?;
        let text_pair = self.graph.concat(text, unconditioned_text, 0)?;
        let text_mask_pair = self.graph.concat(text_mask, text_mask, 0)?;
        let style_key = self.graph.broadcast(
            self.weight("vector_estimator.vector_estimator.Expand_output_0")?,
            style.shape,
        )?;
        let style_key_pair = self.graph.transpose(
            self.graph.concat(
                style_key,
                self.weight("vector_estimator.vector_estimator.tts.ttl.uncond_masker.style_key_special_token")?,
                0,
            )?,
            &[0, 2, 1],
        )?;
        let style_value_pair = self.graph.transpose(
            self.graph.concat(
                style,
                self.weight("vector_estimator.vector_estimator.tts.ttl.uncond_masker.style_value_special_token")?,
                0,
            )?,
            &[0, 2, 1],
        )?;
        let time = self.time_embedding(current_step, total_step)?;
        let latent_pair = self.graph.concat(noisy, noisy, 0)?;
        let input_weight = self
            .weight("vector_estimator.vector_estimator.tts.ttl.vector_field.proj_in.net.weight")?;
        let input_weight = self.graph.reshape(
            input_weight,
            &[input_weight.shape.at(0), input_weight.shape.at(1)],
        )?;
        let mut output = self.graph.linear(
            self.graph
                .contiguous(self.graph.transpose(latent_pair, &[0, 2, 1])?)?,
            input_weight,
            None,
        )?;
        output = self.graph.transpose(output, &[0, 2, 1])?;
        output = self.graph.mul(output, mask_pair)?;
        for group in 0..4 {
            let base = group * 6;
            output = self.convnext(
                output,
                &format!(
                    "vector_estimator.vector_estimator.tts.ttl.vector_field.main_blocks.{base}"
                ),
                &[1, 2, 4, 8],
                true,
                Some(mask_pair),
            )?;
            output = self.add_time(output, time, base + 1)?;
            output = self.convnext(
                output,
                &format!(
                    "vector_estimator.vector_estimator.tts.ttl.vector_field.main_blocks.{}",
                    base + 2
                ),
                &[1],
                false,
                Some(mask_pair),
            )?;
            let text_block = format!(
                "vector_estimator.vector_estimator.tts.ttl.vector_field.main_blocks.{}",
                base + 3
            );
            output = self.graph.mul(output, mask_pair)?;
            let attention = self.cross_attention(
                output,
                text_pair,
                text_pair,
                &format!("{text_block}.attn"),
                8,
                Some(mask_pair),
                false,
                true,
                Some(text_mask_pair),
            )?;
            output =
                self.normalize_vector(self.graph.add(output, attention)?, mask_pair, &text_block)?;
            output = self.convnext(
                output,
                &format!(
                    "vector_estimator.vector_estimator.tts.ttl.vector_field.main_blocks.{}",
                    base + 4
                ),
                &[1],
                false,
                Some(mask_pair),
            )?;
            let style_block = format!(
                "vector_estimator.vector_estimator.tts.ttl.vector_field.main_blocks.{}",
                base + 5
            );
            output = self.graph.mul(output, mask_pair)?;
            let attention = self.cross_attention(
                output,
                style_key_pair,
                style_value_pair,
                &format!("{style_block}.attention"),
                2,
                Some(mask_pair),
                true,
                false,
                None,
            )?;
            output =
                self.normalize_vector(self.graph.add(output, attention)?, mask_pair, &style_block)?;
        }
        output = self.convnext(
            output,
            "vector_estimator.vector_estimator.tts.ttl.vector_field.last_convnext",
            &[1, 1, 1, 1],
            false,
            Some(mask_pair),
        )?;
        let output_weight = self
            .weight("vector_estimator.vector_estimator.tts.ttl.vector_field.proj_out.net.weight")?;
        let output_weight = self.graph.reshape(
            output_weight,
            &[output_weight.shape.at(0), output_weight.shape.at(1)],
        )?;
        output = self.graph.linear(
            self.graph
                .contiguous(self.graph.transpose(output, &[0, 2, 1])?)?,
            output_weight,
            None,
        )?;
        output = self
            .graph
            .mul(self.graph.transpose(output, &[0, 2, 1])?, mask_pair)?;
        let conditional = self.graph.slice(output, 0, 0, 1)?;
        let unconditional = self.graph.slice(output, 0, 1, 1)?;
        let guided = self.graph.sub(
            self.graph.scale(conditional, 4.0)?,
            self.graph.scale(unconditional, 3.0)?,
        )?;
        self.graph.mul(
            self.graph.add(noisy, self.graph.div(guided, total_step)?)?,
            latent_mask,
        )
    }

    fn vocoder_block(&self, input: Tensor, index: usize, dilation: i32) -> Result<Tensor> {
        let prefix = format!("vocoder.tts.ae.decoder.convnext.{index}");
        let padded = self.graph.edge_pad(input, dilation as i64 * 6, 0, 2)?;
        let mut output = self.graph.depthwise_conv1d(
            padded,
            self.weight(&format!("{prefix}.dwconv.net.weight"))?,
            Some(self.weight(&format!("{prefix}.dwconv.net.bias"))?),
            dilation,
        )?;
        output = self.norm(
            self.graph.transpose(output, &[0, 2, 1])?,
            &format!("{prefix}.norm.norm"),
        )?;
        output = self.graph.transpose(output, &[0, 2, 1])?;
        output = self.pointwise(output, &format!("{prefix}.pwconv1"))?;
        output = self.graph.gelu(output)?;
        output = self.pointwise(output, &format!("{prefix}.pwconv2"))?;
        output = self
            .graph
            .mul(output, self.weight(&format!("{prefix}.gamma"))?)?;
        self.graph.add(input, output)
    }

    fn vocoder(
        &self,
        latent: Tensor,
        latent_dim: i64,
        compress: i64,
        base_chunk: i64,
    ) -> Result<Tensor> {
        let frames = latent.shape.at(2) * compress;
        let mut output = self
            .graph
            .div(latent, self.weight("vocoder.tts.ttl.normalizer.scale")?)?;
        output = self.graph.reshape(
            self.graph.contiguous(output)?,
            &[1, latent_dim, compress, latent.shape.at(2)],
        )?;
        output = self.graph.transpose(output, &[0, 1, 3, 2])?;
        output = self
            .graph
            .reshape(self.graph.contiguous(output)?, &[1, latent_dim, frames])?;
        output = self.graph.add(
            self.graph
                .mul(output, self.weight("vocoder.tts.ae.latent_std")?)?,
            self.weight("vocoder.tts.ae.latent_mean")?,
        )?;
        output = self.graph.edge_pad(output, 6, 0, 2)?;
        output = self.conv(output, "vocoder.tts.ae.decoder.embed.net", 1)?;
        let dilations = [1, 2, 4, 1, 2, 4, 1, 1, 1, 1];
        for (index, dilation) in dilations.iter().enumerate() {
            output = self.vocoder_block(output, index, *dilation)?;
        }
        let variance = self.graph.scale_bias(
            self.weight("vocoder.tts.ae.decoder.final_norm.norm.running_var")?,
            1.0,
            1e-5,
        )?;
        let scale = self.graph.div(
            self.weight("vocoder.tts.ae.decoder.final_norm.norm.weight")?,
            self.graph.sqrt(variance)?,
        )?;
        let bias = self.graph.sub(
            self.weight("vocoder.tts.ae.decoder.final_norm.norm.bias")?,
            self.graph.mul(
                self.weight("vocoder.tts.ae.decoder.final_norm.norm.running_mean")?,
                scale,
            )?,
        )?;
        let channels = output.shape.at(1);
        let scale = self.graph.reshape(scale, &[1, channels, 1])?;
        let bias = self.graph.reshape(bias, &[1, channels, 1])?;
        output = self.graph.add(self.graph.mul(output, scale)?, bias)?;
        output = self.graph.edge_pad(output, 2, 0, 2)?;
        output = self.conv(output, "vocoder.tts.ae.decoder.head.layer1.net", 1)?;
        output = self.graph.prelu(
            output,
            self.weight("vocoder.tts.ae.decoder.head.act.weight")?,
        )?;
        output = self.graph.conv1d(
            output,
            self.weight("vocoder.tts.ae.decoder.head.layer2.weight")?,
            None,
            1,
        )?;
        output = self.graph.transpose(output, &[0, 2, 1])?;
        self.graph
            .reshape(self.graph.contiguous(output)?, &[1, frames * base_chunk])
    }
}
