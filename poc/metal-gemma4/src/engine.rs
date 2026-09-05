use std::collections::HashMap;
use std::ffi::c_void;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use metal::{
    Buffer, BufferRef, CommandQueue, ComputeCommandEncoderRef, ComputePipelineState, Device,
    MTLResourceOptions, MTLSize,
};
use serde::Deserialize;

use crate::weights::{Weight, WeightKind, Weights};

const HIDDEN: usize = 1536;
const CONDITION: usize = 256;
const HEADS: usize = 8;
const LAYERS: usize = 35;
const RMS_EPSILON: f32 = 1.0e-6;
const VOCABULARY: usize = 262_144;

#[derive(Deserialize)]
struct Config {
    text_config: TextConfig,
}

#[derive(Deserialize)]
struct TextConfig {
    num_hidden_layers: usize,
    num_kv_shared_layers: usize,
    intermediate_size: usize,
    sliding_window: usize,
    layer_types: Vec<String>,
    use_double_wide_mlp: bool,
    rope_parameters: RopeParameters,
}

#[derive(Deserialize)]
struct RopeParameters {
    full_attention: RopeConfig,
    sliding_attention: RopeConfig,
}

#[derive(Deserialize)]
struct RopeConfig {
    rope_theta: f32,
}

#[derive(Clone)]
struct Linear {
    buffer: Buffer,
    input: usize,
    output: usize,
    row_bytes: usize,
    kind: WeightKind,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct QuantGemvArgs {
    input_size: u32,
    rows: u32,
    row_bytes: u32,
}

#[derive(Clone)]
struct Norm {
    buffer: Buffer,
}

struct Layer {
    full_attention: bool,
    attention_norm: Norm,
    query: Linear,
    query_norm: Norm,
    key: Option<Linear>,
    key_norm: Option<Norm>,
    value: Option<Linear>,
    attention_output: Linear,
    post_attention_norm: Norm,
    ffn_norm: Norm,
    ffn_gate: Linear,
    ffn_up: Linear,
    ffn_down: Linear,
    post_ffn_norm: Norm,
    condition_gate: Linear,
    condition_projection: Linear,
    post_condition_norm: Norm,
    output_scale: f32,
}

struct Cache {
    key: Buffer,
    value: Buffer,
    width: usize,
}

struct Pipelines {
    values: HashMap<&'static str, ComputePipelineState>,
}

impl Pipelines {
    fn load(device: &Device) -> Result<Self> {
        let options = metal::CompileOptions::new();
        let library = device
            .new_library_with_source(include_str!("kernels.metal"), &options)
            .map_err(|error| anyhow!("failed to compile Metal kernels: {error}"))?;
        let mut values = HashMap::new();
        for name in [
            "embedding_scaled",
            "gemv_f16",
            "gemv_q4_k",
            "gemv_q5_k",
            "gemv_q6_k",
            "rms_weighted",
            "rms_weighted_add_scaled",
            "post_attention_ffn_norm",
            "rms",
            "add",
            "scale",
            "geglu",
            "rope_neox",
            "cache_write",
            "attention_decode",
            "argmax_f32",
        ] {
            let function = library
                .get_function(name, None)
                .map_err(|error| anyhow!("failed to load Metal kernel {name}: {error}"))?;
            let pipeline = device
                .new_compute_pipeline_state_with_function(&function)
                .map_err(|error| anyhow!("failed to build Metal pipeline {name}: {error}"))?;
            values.insert(name, pipeline);
        }
        Ok(Self { values })
    }

    fn get(&self, name: &'static str) -> &ComputePipelineState {
        &self.values[name]
    }
}

struct Buffers {
    hidden: Buffer,
    normalized: Buffer,
    residual: Buffer,
    projection: Buffer,
    query: Buffer,
    key: Buffer,
    value: Buffer,
    attended: Buffer,
    ffn_gate: Buffer,
    ffn_up: Buffer,
    condition_gate: Buffer,
    condition_tokens: Buffer,
    condition_projected: Buffer,
    condition: Buffer,
    logits: Buffer,
    token: Buffer,
}

pub struct Engine {
    queue: CommandQueue,
    pipelines: Pipelines,
    token_embedding: Buffer,
    output_projection: Linear,
    condition_embedding: Buffer,
    condition_model_projection: Linear,
    condition_projection_norm: Norm,
    output_norm: Norm,
    rope_factors: Buffer,
    layers: Vec<Layer>,
    caches: Vec<Cache>,
    buffers: Buffers,
    context: usize,
    sliding_window: usize,
    local_theta: f32,
    full_theta: f32,
}

impl Engine {
    pub fn load(model_dir: &Path, context: usize) -> Result<Self> {
        if context == 0 || context > 512 {
            bail!("minimal Metal runtime supports contexts from 1 to 512 tokens");
        }
        let config: Config =
            serde_json::from_reader(std::fs::File::open(model_dir.join("config.json"))?)?;
        validate_config(&config.text_config)?;
        let device = Device::system_default().context("Metal device is unavailable")?;
        eprintln!("Metal device: {}", device.name());
        let pipelines = Pipelines::load(&device)?;
        let model_path = model_dir.join("gemma-4-E2B-it-Q4_K_M.gguf");
        let started = std::time::Instant::now();
        let weights = Weights::load(&device, &model_path)?;
        eprintln!(
            "Loaded {} direct quantized/F16 tensors in {:.2}s",
            weights.len(),
            started.elapsed().as_secs_f32()
        );

        let token_embedding =
            embedding_buffer(weights.get("token_embd.weight")?, HIDDEN, VOCABULARY)?;
        let output_projection = linear(&weights, "token_embd.weight")?;
        let condition_embedding = embedding_buffer(
            weights.get("per_layer_token_embd.weight")?,
            LAYERS * CONDITION,
            VOCABULARY,
        )?;
        let condition_model_projection = linear(&weights, "per_layer_model_proj.weight")?;
        let condition_projection_norm = norm(&weights, "per_layer_proj_norm.weight", CONDITION)?;
        let output_norm = norm(&weights, "output_norm.weight", HIDDEN)?;
        let rope_factors = weights.get("rope_freqs.weight")?.buffer.clone();
        let physical_layers =
            config.text_config.num_hidden_layers - config.text_config.num_kv_shared_layers;
        let mut layers = Vec::with_capacity(LAYERS);
        let mut caches = Vec::with_capacity(physical_layers);
        for layer_index in 0..LAYERS {
            let prefix = format!("blk.{layer_index}");
            let full_attention = config.text_config.layer_types[layer_index] == "full_attention";
            let width = if full_attention { 512 } else { 256 };
            let key = if layer_index < physical_layers {
                Some(linear(&weights, &format!("{prefix}.attn_k.weight"))?)
            } else {
                None
            };
            let key_norm = if layer_index < physical_layers {
                Some(norm(
                    &weights,
                    &format!("{prefix}.attn_k_norm.weight"),
                    width,
                )?)
            } else {
                None
            };
            let value = if layer_index < physical_layers {
                Some(linear(&weights, &format!("{prefix}.attn_v.weight"))?)
            } else {
                None
            };
            if layer_index < physical_layers {
                caches.push(Cache {
                    key: half_buffer(&device, context * width),
                    value: half_buffer(&device, context * width),
                    width,
                });
            }
            layers.push(Layer {
                full_attention,
                attention_norm: norm(&weights, &format!("{prefix}.attn_norm.weight"), HIDDEN)?,
                query: linear(&weights, &format!("{prefix}.attn_q.weight"))?,
                query_norm: norm(&weights, &format!("{prefix}.attn_q_norm.weight"), width)?,
                key,
                key_norm,
                value,
                attention_output: linear(&weights, &format!("{prefix}.attn_output.weight"))?,
                post_attention_norm: norm(
                    &weights,
                    &format!("{prefix}.post_attention_norm.weight"),
                    HIDDEN,
                )?,
                ffn_norm: norm(&weights, &format!("{prefix}.ffn_norm.weight"), HIDDEN)?,
                ffn_gate: linear(&weights, &format!("{prefix}.ffn_gate.weight"))?,
                ffn_up: linear(&weights, &format!("{prefix}.ffn_up.weight"))?,
                ffn_down: linear(&weights, &format!("{prefix}.ffn_down.weight"))?,
                post_ffn_norm: norm(&weights, &format!("{prefix}.post_ffw_norm.weight"), HIDDEN)?,
                condition_gate: linear(&weights, &format!("{prefix}.inp_gate.weight"))?,
                condition_projection: linear(&weights, &format!("{prefix}.proj.weight"))?,
                post_condition_norm: norm(&weights, &format!("{prefix}.post_norm.weight"), HIDDEN)?,
                output_scale: weights.scalar(&format!("{prefix}.layer_output_scale.weight"))?,
            });
        }
        drop(weights);

        let maximum_ffn = config.text_config.intermediate_size
            * if config.text_config.use_double_wide_mlp {
                2
            } else {
                1
            };
        let buffers = Buffers {
            hidden: float_buffer(&device, HIDDEN),
            normalized: float_buffer(&device, maximum_ffn),
            residual: float_buffer(&device, HIDDEN),
            projection: float_buffer(&device, maximum_ffn),
            query: float_buffer(&device, HEADS * 512),
            key: float_buffer(&device, 512),
            value: float_buffer(&device, 512),
            attended: float_buffer(&device, HEADS * 512),
            ffn_gate: float_buffer(&device, maximum_ffn),
            ffn_up: float_buffer(&device, maximum_ffn),
            condition_gate: float_buffer(&device, CONDITION),
            condition_tokens: float_buffer(&device, LAYERS * CONDITION),
            condition_projected: float_buffer(&device, LAYERS * CONDITION),
            condition: float_buffer(&device, LAYERS * CONDITION),
            logits: float_buffer(&device, VOCABULARY),
            token: uint_buffer(&device),
        };

        Ok(Self {
            queue: device.new_command_queue(),
            pipelines,
            token_embedding,
            output_projection,
            condition_embedding,
            condition_model_projection,
            condition_projection_norm,
            output_norm,
            rope_factors,
            layers,
            caches,
            buffers,
            context,
            sliding_window: config.text_config.sliding_window,
            local_theta: config
                .text_config
                .rope_parameters
                .sliding_attention
                .rope_theta,
            full_theta: config.text_config.rope_parameters.full_attention.rope_theta,
        })
    }

    pub fn clear_cache(&self) {
        for cache in &self.caches {
            clear_buffer(&cache.key);
            clear_buffer(&cache.value);
        }
    }

    pub fn decode(&self, token: u32, position: usize) -> Result<u32> {
        if position >= self.context {
            bail!("position {position} exceeds context {}", self.context);
        }
        let command_buffer = self.queue.new_command_buffer();
        let encoder = command_buffer.new_compute_command_encoder();
        self.encode_token(encoder, token, position)?;
        encoder.end_encoding();
        command_buffer.commit();
        command_buffer.wait_until_completed();
        if command_buffer.status() == metal::MTLCommandBufferStatus::Error {
            bail!("Metal command buffer failed");
        }
        Ok(unsafe { *self.buffers.token.contents().cast::<u32>() })
    }

    fn encode_token(
        &self,
        encoder: &ComputeCommandEncoderRef,
        token: u32,
        position: usize,
    ) -> Result<()> {
        self.encode_embedding(
            encoder,
            &self.token_embedding,
            &self.buffers.hidden,
            token,
            (HIDDEN as f32).sqrt(),
            HIDDEN,
        );
        self.encode_embedding(
            encoder,
            &self.condition_embedding,
            &self.buffers.condition_tokens,
            token,
            (CONDITION as f32).sqrt(),
            LAYERS * CONDITION,
        );
        self.encode_gemv(
            encoder,
            &self.condition_model_projection,
            &self.buffers.hidden,
            &self.buffers.condition_projected,
        );
        self.encode_scale(
            encoder,
            &self.buffers.condition_projected,
            &self.buffers.condition_projected,
            1.0 / (HIDDEN as f32).sqrt(),
            LAYERS * CONDITION,
        );
        self.encode_rms_weighted(
            encoder,
            &self.buffers.condition_projected,
            &self.condition_projection_norm,
            &self.buffers.condition_projected,
            LAYERS,
            CONDITION,
        );
        self.encode_add(
            encoder,
            &self.buffers.condition_projected,
            &self.buffers.condition_tokens,
            &self.buffers.condition,
            LAYERS * CONDITION,
        );
        self.encode_scale(
            encoder,
            &self.buffers.condition,
            &self.buffers.condition,
            1.0 / 2.0_f32.sqrt(),
            LAYERS * CONDITION,
        );

        for (layer_index, layer) in self.layers.iter().enumerate() {
            let width = if layer.full_attention { 512 } else { 256 };
            self.encode_rms_weighted(
                encoder,
                &self.buffers.hidden,
                &layer.attention_norm,
                &self.buffers.normalized,
                1,
                HIDDEN,
            );
            self.encode_gemv(
                encoder,
                &layer.query,
                &self.buffers.normalized,
                &self.buffers.query,
            );
            self.encode_rms_weighted(
                encoder,
                &self.buffers.query,
                &layer.query_norm,
                &self.buffers.query,
                HEADS,
                width,
            );
            self.encode_rope(
                encoder,
                &self.buffers.query,
                HEADS,
                width,
                position,
                if layer.full_attention {
                    self.full_theta
                } else {
                    self.local_theta
                },
                layer.full_attention,
            );

            let cache_index = if layer_index < self.caches.len() {
                let key = layer
                    .key
                    .as_ref()
                    .context("physical layer has no key projection")?;
                let value = layer
                    .value
                    .as_ref()
                    .context("physical layer has no value projection")?;
                let key_norm = layer
                    .key_norm
                    .as_ref()
                    .context("physical layer has no key norm")?;
                self.encode_gemv(encoder, key, &self.buffers.normalized, &self.buffers.key);
                self.encode_gemv(
                    encoder,
                    value,
                    &self.buffers.normalized,
                    &self.buffers.value,
                );
                self.encode_rms_weighted(
                    encoder,
                    &self.buffers.key,
                    key_norm,
                    &self.buffers.key,
                    1,
                    width,
                );
                self.encode_rms(encoder, &self.buffers.value, &self.buffers.value, 1, width);
                self.encode_rope(
                    encoder,
                    &self.buffers.key,
                    1,
                    width,
                    position,
                    if layer.full_attention {
                        self.full_theta
                    } else {
                        self.local_theta
                    },
                    layer.full_attention,
                );
                self.encode_cache_write(
                    encoder,
                    &self.buffers.key,
                    &self.caches[layer_index].key,
                    position,
                    width,
                );
                self.encode_cache_write(
                    encoder,
                    &self.buffers.value,
                    &self.caches[layer_index].value,
                    position,
                    width,
                );
                layer_index
            } else if layer.full_attention {
                self.caches.len() - 1
            } else {
                self.caches.len() - 2
            };
            let cache = &self.caches[cache_index];
            if cache.width != width {
                bail!("shared KV cache width mismatch at layer {layer_index}");
            }
            self.encode_attention(
                encoder,
                &self.buffers.query,
                cache,
                &self.buffers.attended,
                position + 1,
                if layer.full_attention {
                    0
                } else {
                    self.sliding_window
                },
            );
            self.encode_gemv(
                encoder,
                &layer.attention_output,
                &self.buffers.attended,
                &self.buffers.projection,
            );
            self.encode_post_attention_ffn_norm(
                encoder,
                &self.buffers.projection,
                &layer.post_attention_norm,
                &self.buffers.hidden,
                &self.buffers.residual,
                &layer.ffn_norm,
                &self.buffers.normalized,
            );
            self.encode_gemv(
                encoder,
                &layer.ffn_gate,
                &self.buffers.normalized,
                &self.buffers.ffn_gate,
            );
            self.encode_gemv(
                encoder,
                &layer.ffn_up,
                &self.buffers.normalized,
                &self.buffers.ffn_up,
            );
            self.encode_geglu(
                encoder,
                &self.buffers.ffn_gate,
                0,
                &self.buffers.ffn_up,
                0,
                &self.buffers.ffn_gate,
                layer.ffn_gate.output,
            );
            self.encode_gemv(
                encoder,
                &layer.ffn_down,
                &self.buffers.ffn_gate,
                &self.buffers.projection,
            );
            self.encode_rms_weighted_add_scaled(
                encoder,
                &self.buffers.projection,
                &layer.post_ffn_norm,
                &self.buffers.residual,
                &self.buffers.hidden,
                1.0,
            );
            self.encode_gemv(
                encoder,
                &layer.condition_gate,
                &self.buffers.hidden,
                &self.buffers.condition_gate,
            );
            self.encode_geglu(
                encoder,
                &self.buffers.condition_gate,
                0,
                &self.buffers.condition,
                layer_index * CONDITION * std::mem::size_of::<f32>(),
                &self.buffers.condition_gate,
                CONDITION,
            );
            self.encode_gemv(
                encoder,
                &layer.condition_projection,
                &self.buffers.condition_gate,
                &self.buffers.projection,
            );
            self.encode_rms_weighted_add_scaled(
                encoder,
                &self.buffers.projection,
                &layer.post_condition_norm,
                &self.buffers.hidden,
                &self.buffers.hidden,
                layer.output_scale,
            );
        }

        self.encode_rms_weighted(
            encoder,
            &self.buffers.hidden,
            &self.output_norm,
            &self.buffers.normalized,
            1,
            HIDDEN,
        );
        self.encode_gemv(
            encoder,
            &self.output_projection,
            &self.buffers.normalized,
            &self.buffers.logits,
        );
        self.encode_argmax(
            encoder,
            &self.buffers.logits,
            &self.buffers.token,
            VOCABULARY,
        );
        Ok(())
    }

    fn encode_embedding(
        &self,
        encoder: &ComputeCommandEncoderRef,
        weight: &BufferRef,
        output: &BufferRef,
        token: u32,
        factor: f32,
        width: usize,
    ) {
        encoder.set_compute_pipeline_state(self.pipelines.get("embedding_scaled"));
        encoder.set_buffer(0, Some(weight), 0);
        encoder.set_buffer(1, Some(output), 0);
        set_value(encoder, 2, &token);
        set_value(encoder, 3, &factor);
        set_value(encoder, 4, &(width as u32));
        dispatch_elements(encoder, width, 256);
    }

    fn encode_gemv(
        &self,
        encoder: &ComputeCommandEncoderRef,
        linear: &Linear,
        input: &BufferRef,
        output: &BufferRef,
    ) {
        let pipeline = match linear.kind {
            WeightKind::F16 => "gemv_f16",
            WeightKind::Q4K => "gemv_q4_k",
            WeightKind::Q5K => "gemv_q5_k",
            WeightKind::Q6K => "gemv_q6_k",
        };
        encoder.set_compute_pipeline_state(self.pipelines.get(pipeline));
        encoder.set_buffer(0, Some(&linear.buffer), 0);
        encoder.set_buffer(1, Some(input), 0);
        encoder.set_buffer(2, Some(output), 0);
        if linear.kind == WeightKind::F16 {
            set_value(encoder, 3, &(linear.output as u32));
            set_value(encoder, 4, &(linear.input as u32));
            encoder.dispatch_thread_groups(size(linear.output.div_ceil(4)), size(128));
        } else {
            let arguments = QuantGemvArgs {
                input_size: linear.input as u32,
                rows: linear.output as u32,
                row_bytes: linear.row_bytes as u32,
            };
            set_value(encoder, 3, &arguments);
            encoder.dispatch_thread_groups(size(linear.output.div_ceil(4)), size(64));
        }
    }

    fn encode_rms_weighted(
        &self,
        encoder: &ComputeCommandEncoderRef,
        input: &BufferRef,
        norm: &Norm,
        output: &BufferRef,
        rows: usize,
        width: usize,
    ) {
        encoder.set_compute_pipeline_state(self.pipelines.get("rms_weighted"));
        encoder.set_buffer(0, Some(input), 0);
        encoder.set_buffer(1, Some(&norm.buffer), 0);
        encoder.set_buffer(2, Some(output), 0);
        set_value(encoder, 3, &(rows as u32));
        set_value(encoder, 4, &(width as u32));
        set_value(encoder, 5, &RMS_EPSILON);
        encoder.dispatch_thread_groups(size(rows), size(256));
    }

    fn encode_rms_weighted_add_scaled(
        &self,
        encoder: &ComputeCommandEncoderRef,
        input: &BufferRef,
        norm: &Norm,
        residual: &BufferRef,
        output: &BufferRef,
        factor: f32,
    ) {
        encoder.set_compute_pipeline_state(self.pipelines.get("rms_weighted_add_scaled"));
        encoder.set_buffer(0, Some(input), 0);
        encoder.set_buffer(1, Some(&norm.buffer), 0);
        encoder.set_buffer(2, Some(residual), 0);
        encoder.set_buffer(3, Some(output), 0);
        set_value(encoder, 4, &1u32);
        set_value(encoder, 5, &(HIDDEN as u32));
        set_value(encoder, 6, &RMS_EPSILON);
        set_value(encoder, 7, &factor);
        encoder.dispatch_thread_groups(size(1), size(256));
    }

    fn encode_post_attention_ffn_norm(
        &self,
        encoder: &ComputeCommandEncoderRef,
        attention: &BufferRef,
        post_attention_norm: &Norm,
        hidden: &BufferRef,
        residual: &BufferRef,
        ffn_norm: &Norm,
        normalized: &BufferRef,
    ) {
        encoder.set_compute_pipeline_state(self.pipelines.get("post_attention_ffn_norm"));
        encoder.set_buffer(0, Some(attention), 0);
        encoder.set_buffer(1, Some(&post_attention_norm.buffer), 0);
        encoder.set_buffer(2, Some(hidden), 0);
        encoder.set_buffer(3, Some(residual), 0);
        encoder.set_buffer(4, Some(&ffn_norm.buffer), 0);
        encoder.set_buffer(5, Some(normalized), 0);
        set_value(encoder, 6, &(HIDDEN as u32));
        set_value(encoder, 7, &RMS_EPSILON);
        encoder.dispatch_thread_groups(size(1), size(256));
    }

    fn encode_rms(
        &self,
        encoder: &ComputeCommandEncoderRef,
        input: &BufferRef,
        output: &BufferRef,
        rows: usize,
        width: usize,
    ) {
        encoder.set_compute_pipeline_state(self.pipelines.get("rms"));
        encoder.set_buffer(0, Some(input), 0);
        encoder.set_buffer(1, Some(output), 0);
        set_value(encoder, 2, &(rows as u32));
        set_value(encoder, 3, &(width as u32));
        set_value(encoder, 4, &RMS_EPSILON);
        encoder.dispatch_thread_groups(size(rows), size(256));
    }

    fn encode_add(
        &self,
        encoder: &ComputeCommandEncoderRef,
        left: &BufferRef,
        right: &BufferRef,
        output: &BufferRef,
        count: usize,
    ) {
        encoder.set_compute_pipeline_state(self.pipelines.get("add"));
        encoder.set_buffer(0, Some(left), 0);
        encoder.set_buffer(1, Some(right), 0);
        encoder.set_buffer(2, Some(output), 0);
        set_value(encoder, 3, &(count as u32));
        dispatch_elements(encoder, count, 256);
    }

    fn encode_scale(
        &self,
        encoder: &ComputeCommandEncoderRef,
        input: &BufferRef,
        output: &BufferRef,
        factor: f32,
        count: usize,
    ) {
        encoder.set_compute_pipeline_state(self.pipelines.get("scale"));
        encoder.set_buffer(0, Some(input), 0);
        encoder.set_buffer(1, Some(output), 0);
        set_value(encoder, 2, &factor);
        set_value(encoder, 3, &(count as u32));
        dispatch_elements(encoder, count, 256);
    }

    fn encode_geglu(
        &self,
        encoder: &ComputeCommandEncoderRef,
        gate: &BufferRef,
        gate_offset: usize,
        up: &BufferRef,
        up_offset: usize,
        output: &BufferRef,
        count: usize,
    ) {
        encoder.set_compute_pipeline_state(self.pipelines.get("geglu"));
        encoder.set_buffer(0, Some(gate), gate_offset as u64);
        encoder.set_buffer(1, Some(up), up_offset as u64);
        encoder.set_buffer(2, Some(output), 0);
        set_value(encoder, 3, &(count as u32));
        dispatch_elements(encoder, count, 256);
    }

    fn encode_rope(
        &self,
        encoder: &ComputeCommandEncoderRef,
        data: &BufferRef,
        heads: usize,
        width: usize,
        position: usize,
        theta: f32,
        has_factors: bool,
    ) {
        encoder.set_compute_pipeline_state(self.pipelines.get("rope_neox"));
        encoder.set_buffer(0, Some(data), 0);
        set_value(encoder, 1, &(heads as u32));
        set_value(encoder, 2, &(width as u32));
        set_value(encoder, 3, &(width as u32));
        set_value(encoder, 4, &(position as i32));
        set_value(encoder, 5, &theta);
        encoder.set_buffer(6, Some(&self.rope_factors), 0);
        set_value(encoder, 7, &(has_factors as u32));
        dispatch_elements(encoder, heads * width / 2, 256);
    }

    fn encode_cache_write(
        &self,
        encoder: &ComputeCommandEncoderRef,
        input: &BufferRef,
        cache: &BufferRef,
        position: usize,
        width: usize,
    ) {
        encoder.set_compute_pipeline_state(self.pipelines.get("cache_write"));
        encoder.set_buffer(0, Some(input), 0);
        encoder.set_buffer(1, Some(cache), 0);
        set_value(encoder, 2, &(position as u32));
        set_value(encoder, 3, &(width as u32));
        dispatch_elements(encoder, width, 256);
    }

    fn encode_attention(
        &self,
        encoder: &ComputeCommandEncoderRef,
        query: &BufferRef,
        cache: &Cache,
        output: &BufferRef,
        active_length: usize,
        window: usize,
    ) {
        encoder.set_compute_pipeline_state(self.pipelines.get("attention_decode"));
        encoder.set_buffer(0, Some(query), 0);
        encoder.set_buffer(1, Some(&cache.key), 0);
        encoder.set_buffer(2, Some(&cache.value), 0);
        encoder.set_buffer(3, Some(output), 0);
        set_value(encoder, 4, &(active_length as u32));
        set_value(encoder, 5, &(cache.width as u32));
        set_value(encoder, 6, &(window as u32));
        encoder.dispatch_thread_groups(size(HEADS), size(256));
    }

    fn encode_argmax(
        &self,
        encoder: &ComputeCommandEncoderRef,
        input: &BufferRef,
        output: &BufferRef,
        count: usize,
    ) {
        encoder.set_compute_pipeline_state(self.pipelines.get("argmax_f32"));
        encoder.set_buffer(0, Some(input), 0);
        encoder.set_buffer(1, Some(output), 0);
        set_value(encoder, 2, &(count as u32));
        encoder.dispatch_thread_groups(size(1), size(256));
    }
}

fn validate_config(config: &TextConfig) -> Result<()> {
    if config.num_hidden_layers != LAYERS
        || config.num_hidden_layers - config.num_kv_shared_layers != 15
        || config.layer_types.len() != LAYERS
        || config.intermediate_size != 6144
    {
        bail!("unsupported Gemma 4 configuration");
    }
    Ok(())
}

fn linear(weights: &Weights, name: &str) -> Result<Linear> {
    let weight = weights.get(name)?;
    if weight.dimensions[2] != 1 || weight.dimensions[3] != 1 {
        bail!("linear weight {name} is not a matrix");
    }
    let row_bytes = match weight.kind {
        WeightKind::F16 => weight.dimensions[0] * 2,
        WeightKind::Q4K => weight.dimensions[0] / 256 * 144,
        WeightKind::Q5K => weight.dimensions[0] / 256 * 176,
        WeightKind::Q6K => weight.dimensions[0] / 256 * 210,
    };
    Ok(Linear {
        buffer: weight.buffer.clone(),
        input: weight.dimensions[0],
        output: weight.dimensions[1],
        row_bytes,
        kind: weight.kind,
    })
}

fn norm(weights: &Weights, name: &str, width: usize) -> Result<Norm> {
    let weight = weights.get(name)?;
    if weight.elements != width {
        bail!(
            "normalization weight {name} has {} values, expected {width}",
            weight.elements
        );
    }
    Ok(Norm {
        buffer: weight.buffer.clone(),
    })
}

fn embedding_buffer(weight: &Weight, width: usize, rows: usize) -> Result<Buffer> {
    if weight.dimensions[0] != width || weight.dimensions[1] != rows {
        bail!(
            "weight shape is [{}, {}], expected [{width}, {rows}]",
            weight.dimensions[0],
            weight.dimensions[1]
        );
    }
    weight
        .embedding_buffer
        .as_ref()
        .or_else(|| (weight.kind == WeightKind::F16).then_some(&weight.buffer))
        .cloned()
        .context("embedding weight has no F16 gather buffer")
}

fn float_buffer(device: &Device, count: usize) -> Buffer {
    device.new_buffer(
        (count * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    )
}

fn half_buffer(device: &Device, count: usize) -> Buffer {
    let buffer = device.new_buffer(
        (count * std::mem::size_of::<u16>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    clear_buffer(&buffer);
    buffer
}

fn uint_buffer(device: &Device) -> Buffer {
    device.new_buffer(
        std::mem::size_of::<u32>() as u64,
        MTLResourceOptions::StorageModeShared,
    )
}

fn clear_buffer(buffer: &BufferRef) {
    unsafe { std::ptr::write_bytes(buffer.contents(), 0, buffer.length() as usize) };
}

fn set_value<T>(encoder: &ComputeCommandEncoderRef, index: u64, value: &T) {
    encoder.set_bytes(
        index,
        std::mem::size_of::<T>() as u64,
        (value as *const T).cast::<c_void>(),
    );
}

fn dispatch_elements(encoder: &ComputeCommandEncoderRef, count: usize, threads: usize) {
    encoder.dispatch_thread_groups(size(count.div_ceil(threads)), size(threads));
}

fn size(width: usize) -> MTLSize {
    MTLSize {
        width: width as u64,
        height: 1,
        depth: 1,
    }
}
