use anyhow::{bail, Context, Result};
use ggml_runtime::{
    Backend, BackendKind, Graph, GraphBuilder, Shape, Tensor, TensorStorage, Weights,
};
use serde::Deserialize;
use std::path::Path;
use std::time::Instant;

const GRAPH_METADATA_BYTES: usize = 64 * 1024 * 1024;
const SLIDING_ATTENTION: &str = "sliding_attention";
const FULL_ATTENTION: &str = "full_attention";

#[derive(Debug, Deserialize)]
struct Config {
    model_type: String,
    text_config: TextConfig,
}

#[derive(Debug, Deserialize)]
struct TextConfig {
    vocab_size: usize,
    hidden_size: usize,
    hidden_size_per_layer_input: usize,
    intermediate_size: usize,
    num_hidden_layers: usize,
    num_attention_heads: usize,
    num_key_value_heads: usize,
    num_kv_shared_layers: usize,
    head_dim: usize,
    global_head_dim: usize,
    max_position_embeddings: usize,
    sliding_window: usize,
    rms_norm_eps: f32,
    final_logit_softcapping: f32,
    layer_types: Vec<String>,
    rope_parameters: RopeParameters,
    use_double_wide_mlp: bool,
    enable_moe_block: bool,
}

#[derive(Debug, Deserialize)]
struct RopeParameters {
    full_attention: RopeConfig,
    sliding_attention: RopeConfig,
}

#[derive(Debug, Deserialize)]
struct RopeConfig {
    rope_theta: f32,
    #[serde(default = "one")]
    partial_rotary_factor: f32,
}

fn one() -> f32 {
    1.0
}

struct Execution {
    graph: Graph,
    token_ids: Tensor,
    positions: Tensor,
    sliding_mask: Tensor,
    full_mask: Tensor,
    token_count: usize,
    capacity: usize,
    sliding_window: usize,
    flash_attention: bool,
}

impl Execution {
    fn run(&self, backend: &Backend, token_ids: &[i32], positions: &[i32]) -> Result<()> {
        if token_ids.len() != self.token_count || positions.len() != self.token_count {
            bail!(
                "execution requires {} tokens and positions",
                self.token_count
            );
        }
        if positions.iter().any(|position| {
            *position < 0 || usize::try_from(*position).map_or(true, |value| value >= self.capacity)
        }) {
            bail!("position exceeds benchmark cache capacity");
        }
        self.graph.set_i32(self.token_ids, token_ids)?;
        self.graph.set_i32(self.positions, positions)?;
        if self.flash_attention {
            self.graph.set_f16_bits(
                self.sliding_mask,
                &attention_mask_f16(positions, self.capacity, Some(self.sliding_window)),
            )?;
            self.graph.set_f16_bits(
                self.full_mask,
                &attention_mask_f16(positions, self.capacity, None),
            )?;
        } else {
            self.graph.set_f32(
                self.sliding_mask,
                &attention_mask(positions, self.capacity, Some(self.sliding_window)),
            )?;
            self.graph.set_f32(
                self.full_mask,
                &attention_mask(positions, self.capacity, None),
            )?;
        }
        self.graph.compute(backend)
    }
}

pub struct Model {
    prompt: Execution,
    generation: Execution,
    prompt_cache: TensorStorage,
    generation_cache: TensorStorage,
    _weights: Weights,
    config: Config,
    backend: Backend,
}

impl Model {
    pub fn load(
        model_dir: &Path,
        prompt_tokens: usize,
        generation_tokens: usize,
        flash_attention: bool,
    ) -> Result<Self> {
        if prompt_tokens == 0 || generation_tokens == 0 {
            bail!("prompt and generation token counts must be non-zero");
        }
        let config: Config =
            serde_json::from_reader(std::fs::File::open(model_dir.join("config.json"))?)?;
        validate_config(&config)?;
        if prompt_tokens > config.text_config.max_position_embeddings
            || generation_tokens > config.text_config.max_position_embeddings
        {
            bail!("benchmark token count exceeds the model context window");
        }

        let backend = Backend::new(BackendKind::Metal)?;
        let started = Instant::now();
        let weights = Weights::load(&model_dir.join("gemma-4-E2B-it-Q4_K_M.gguf"), &backend)?;
        eprintln!(
            "Loaded {} Gemma 4 tensors on Metal in {:.2}s",
            weights.len(),
            started.elapsed().as_secs_f32()
        );

        let prompt_cache =
            TensorStorage::f16(&backend, &cache_shapes(&config.text_config, prompt_tokens))?;
        let generation_cache = TensorStorage::f16(
            &backend,
            &cache_shapes(&config.text_config, generation_tokens),
        )?;
        let prompt = build_execution(
            &config.text_config,
            &weights,
            &prompt_cache,
            &backend,
            prompt_tokens,
            prompt_tokens,
            flash_attention,
        )?;
        let generation = build_execution(
            &config.text_config,
            &weights,
            &generation_cache,
            &backend,
            1,
            generation_tokens,
            flash_attention,
        )?;

        Ok(Self {
            prompt,
            generation,
            prompt_cache,
            generation_cache,
            _weights: weights,
            config,
            backend,
        })
    }

    pub fn vocab_size(&self) -> usize {
        self.config.text_config.vocab_size
    }

    pub fn run_prompt(&self, token_ids: &[i32]) -> Result<()> {
        let positions = (0..token_ids.len())
            .map(|position| i32::try_from(position).context("prompt position exceeds i32"))
            .collect::<Result<Vec<_>>>()?;
        self.prompt.run(&self.backend, token_ids, &positions)
    }

    pub fn run_generation_token(&self, token_id: i32, position: usize) -> Result<()> {
        self.generation.run(
            &self.backend,
            &[token_id],
            &[i32::try_from(position).context("generation position exceeds i32")?],
        )
    }

    pub fn generation_logits(&self) -> Result<Vec<f32>> {
        self.generation.graph.output_f32()
    }

    pub fn clear_prompt_cache(&self) {
        self.prompt_cache.clear();
        self.backend.synchronize();
    }

    pub fn clear_generation_cache(&self) {
        self.generation_cache.clear();
        self.backend.synchronize();
    }
}

fn cache_shapes(config: &TextConfig, capacity: usize) -> Vec<Vec<i64>> {
    let first_shared_layer = config.num_hidden_layers - config.num_kv_shared_layers;
    let mut shapes = Vec::with_capacity(first_shared_layer * 2);
    for layer in 0..first_shared_layer {
        let head_dim = head_dim(config, &config.layer_types[layer]);
        let shape = vec![
            1,
            config.num_key_value_heads as i64,
            capacity as i64,
            head_dim as i64,
        ];
        shapes.push(shape.clone());
        shapes.push(shape);
    }
    shapes
}

fn build_execution(
    config: &TextConfig,
    weights: &Weights,
    cache: &TensorStorage,
    backend: &Backend,
    token_count: usize,
    capacity: usize,
    flash_attention: bool,
) -> Result<Execution> {
    let graph = GraphBuilder::new(GRAPH_METADATA_BYTES)?;
    let token_ids = graph.input_i32(&[1, token_count as i64])?;
    let positions = graph.input_i32(&[token_count as i64])?;
    let mask_shape = [1, 1, token_count as i64, capacity as i64];
    let sliding_mask = if flash_attention {
        graph.input_f16(&mask_shape)?
    } else {
        graph.input_f32(&mask_shape)?
    };
    let full_mask = if flash_attention {
        graph.input_f16(&mask_shape)?
    } else {
        graph.input_f32(&mask_shape)?
    };
    let mut hidden = graph.scale(
        graph.embedding(token_ids, weights.get("token_embd.weight")?)?,
        (config.hidden_size as f32).sqrt(),
    )?;
    let per_layer_tokens = graph.scale(
        graph.embedding(token_ids, weights.get("per_layer_token_embd.weight")?)?,
        (config.hidden_size_per_layer_input as f32).sqrt(),
    )?;
    let per_layer_tokens = graph.reshape(
        per_layer_tokens,
        &[
            1,
            token_count as i64,
            config.num_hidden_layers as i64,
            config.hidden_size_per_layer_input as i64,
        ],
    )?;
    let projected = graph.scale(
        graph.linear(hidden, weights.get("per_layer_model_proj.weight")?, None)?,
        1.0 / (config.hidden_size as f32).sqrt(),
    )?;
    let projected = graph.reshape(
        projected,
        &[
            1,
            token_count as i64,
            config.num_hidden_layers as i64,
            config.hidden_size_per_layer_input as i64,
        ],
    )?;
    let projected = weighted_rms_norm(
        &graph,
        projected,
        weights.get("per_layer_proj_norm.weight")?,
        config.rms_norm_eps,
    )?;
    let per_layer = graph.scale(
        graph.add(projected, per_layer_tokens)?,
        1.0 / 2.0_f32.sqrt(),
    )?;
    let per_layer = graph.contiguous(graph.transpose(per_layer, &[0, 2, 1, 3])?)?;

    let first_shared_layer = config.num_hidden_layers - config.num_kv_shared_layers;
    let sliding_source_layer = first_shared_layer - 2;
    let full_source_layer = first_shared_layer - 1;
    let mut cached_key_values = vec![None; first_shared_layer];

    for layer in 0..config.num_hidden_layers {
        let layer_type = &config.layer_types[layer];
        let prefix = format!("blk.{layer}");
        let per_layer_input = graph.reshape(
            graph.slice(per_layer, 1, layer as i64, 1)?,
            &[
                1,
                token_count as i64,
                config.hidden_size_per_layer_input as i64,
            ],
        )?;
        let normalized = weighted_rms_norm(
            &graph,
            hidden,
            weights.get(&format!("{prefix}.attn_norm.weight"))?,
            config.rms_norm_eps,
        )?;
        let cache_mode = if layer < first_shared_layer {
            CacheMode::Write {
                key: cache.get(layer * 2)?,
                value: cache.get(layer * 2 + 1)?,
            }
        } else {
            let source_layer = if layer_type == SLIDING_ATTENTION {
                sliding_source_layer
            } else {
                full_source_layer
            };
            let (key, value) = cached_key_values[source_layer]
                .context("shared Gemma 4 KV source was not built")?;
            CacheMode::Read { key, value }
        };
        let (rope_config, rope_factors, mask) = if layer_type == SLIDING_ATTENTION {
            (
                &config.rope_parameters.sliding_attention,
                None,
                sliding_mask,
            )
        } else {
            (
                &config.rope_parameters.full_attention,
                Some(weights.get("rope_freqs.weight")?),
                full_mask,
            )
        };
        let (attention, updated_cache) = attention(
            &graph,
            config,
            weights,
            normalized,
            &prefix,
            layer_type,
            positions,
            rope_config,
            rope_factors,
            mask,
            cache_mode,
            token_count,
            capacity,
            flash_attention,
        )?;
        if let Some(key_values) = updated_cache {
            cached_key_values[layer] = Some(key_values);
        }
        let attention = weighted_rms_norm(
            &graph,
            attention,
            weights.get(&format!("{prefix}.post_attention_norm.weight"))?,
            config.rms_norm_eps,
        )?;
        let attention_output = graph.add(attention, hidden)?;
        let normalized = weighted_rms_norm(
            &graph,
            attention_output,
            weights.get(&format!("{prefix}.ffn_norm.weight"))?,
            config.rms_norm_eps,
        )?;
        let feed_forward = feed_forward(&graph, config, weights, normalized, &prefix, layer)?;
        let feed_forward = weighted_rms_norm(
            &graph,
            feed_forward,
            weights.get(&format!("{prefix}.post_ffw_norm.weight"))?,
            config.rms_norm_eps,
        )?;
        let mut output = graph.add(feed_forward, attention_output)?;
        let gate = graph.linear(
            output,
            weights.get(&format!("{prefix}.inp_gate.weight"))?,
            None,
        )?;
        let projected = graph.linear(
            graph.geglu(gate, per_layer_input)?,
            weights.get(&format!("{prefix}.proj.weight"))?,
            None,
        )?;
        let projected = weighted_rms_norm(
            &graph,
            projected,
            weights.get(&format!("{prefix}.post_norm.weight"))?,
            config.rms_norm_eps,
        )?;
        output = graph.add(projected, output)?;
        hidden = graph.mul(
            output,
            weights.get(&format!("{prefix}.layer_output_scale.weight"))?,
        )?;
    }

    let hidden = graph.slice(hidden, 1, token_count as i64 - 1, 1)?;
    let normalized = weighted_rms_norm(
        &graph,
        hidden,
        weights.get("output_norm.weight")?,
        config.rms_norm_eps,
    )?;
    let logits = graph.linear(normalized, weights.get("token_embd.weight")?, None)?;
    let logits = graph.scale(
        graph.tanh(graph.scale(logits, 1.0 / config.final_logit_softcapping)?)?,
        config.final_logit_softcapping,
    )?;
    let logits = graph.contiguous(logits)?;
    let computation = graph.finish(logits, backend)?;

    Ok(Execution {
        graph: computation,
        token_ids,
        positions,
        sliding_mask,
        full_mask,
        token_count,
        capacity,
        sliding_window: config.sliding_window,
        flash_attention,
    })
}

#[derive(Clone, Copy)]
enum CacheMode {
    Write { key: Tensor, value: Tensor },
    Read { key: Tensor, value: Tensor },
}

#[allow(clippy::too_many_arguments)]
fn attention(
    graph: &GraphBuilder,
    config: &TextConfig,
    weights: &Weights,
    input: Tensor,
    prefix: &str,
    layer_type: &str,
    positions: Tensor,
    rope_config: &RopeConfig,
    rope_factors: Option<Tensor>,
    mask: Tensor,
    cache_mode: CacheMode,
    token_count: usize,
    capacity: usize,
    flash_attention: bool,
) -> Result<(Tensor, Option<(Tensor, Tensor)>)> {
    let current_head_dim = head_dim(config, layer_type);
    let query = graph.linear(
        input,
        weights.get(&format!("{prefix}.attn_q.weight"))?,
        None,
    )?;
    let query = graph.reshape(
        graph.contiguous(query)?,
        &[
            1,
            token_count as i64,
            config.num_attention_heads as i64,
            current_head_dim as i64,
        ],
    )?;
    let query = weighted_rms_norm(
        graph,
        query,
        weights.get(&format!("{prefix}.attn_q_norm.weight"))?,
        config.rms_norm_eps,
    )?;
    let query = graph.transpose(
        apply_rope(
            graph,
            query,
            positions,
            current_head_dim,
            rope_config,
            config.max_position_embeddings,
            rope_factors,
        )?,
        &[0, 2, 1, 3],
    )?;

    let (key, value, updated_cache) = match cache_mode {
        CacheMode::Write {
            key: key_cache,
            value: value_cache,
        } => {
            let key = graph.linear(
                input,
                weights.get(&format!("{prefix}.attn_k.weight"))?,
                None,
            )?;
            let value = graph.linear(
                input,
                weights.get(&format!("{prefix}.attn_v.weight"))?,
                None,
            )?;
            let shape = [
                1,
                token_count as i64,
                config.num_key_value_heads as i64,
                current_head_dim as i64,
            ];
            let key = graph.reshape(graph.contiguous(key)?, &shape)?;
            let value = graph.reshape(graph.contiguous(value)?, &shape)?;
            let key = weighted_rms_norm(
                graph,
                key,
                weights.get(&format!("{prefix}.attn_k_norm.weight"))?,
                config.rms_norm_eps,
            )?;
            let value = graph.rms_norm(value, config.rms_norm_eps)?;
            let key = graph.transpose(
                apply_rope(
                    graph,
                    key,
                    positions,
                    current_head_dim,
                    rope_config,
                    config.max_position_embeddings,
                    rope_factors,
                )?,
                &[0, 2, 1, 3],
            )?;
            let value = graph.transpose(value, &[0, 2, 1, 3])?;
            let key = graph.set_rows(key_cache, key, positions)?;
            let value = graph.set_rows(value_cache, value, positions)?;
            (key, value, Some((key, value)))
        }
        CacheMode::Read { key, value } => (key, value, None),
    };

    let attended = if flash_attention {
        graph.flash_attention(query, key, value, mask, 1.0)?
    } else {
        let attention_shape = Shape::new(&[
            1,
            config.num_attention_heads as i64,
            capacity as i64,
            current_head_dim as i64,
        ])?;
        let key_for_attention = graph.broadcast(graph.cast_f32(key)?, attention_shape)?;
        let value_for_attention = graph.broadcast(graph.cast_f32(value)?, attention_shape)?;
        let key_for_attention = graph.transpose(key_for_attention, &[0, 1, 3, 2])?;
        let scores = graph.matmul(query, key_for_attention)?;
        let probabilities = graph.softmax(graph.add(scores, mask)?)?;
        let attended = graph.matmul(probabilities, value_for_attention)?;
        graph.transpose(attended, &[0, 2, 1, 3])?
    };
    let attended = graph.reshape(
        graph.contiguous(attended)?,
        &[
            1,
            token_count as i64,
            (config.num_attention_heads * current_head_dim) as i64,
        ],
    )?;
    let output = graph.linear(
        attended,
        weights.get(&format!("{prefix}.attn_output.weight"))?,
        None,
    )?;
    Ok((output, updated_cache))
}

fn feed_forward(
    graph: &GraphBuilder,
    config: &TextConfig,
    weights: &Weights,
    input: Tensor,
    prefix: &str,
    layer: usize,
) -> Result<Tensor> {
    let first_shared_layer = config.num_hidden_layers - config.num_kv_shared_layers;
    let intermediate_size = if config.use_double_wide_mlp && layer >= first_shared_layer {
        config.intermediate_size * 2
    } else {
        config.intermediate_size
    };
    let gate = graph.linear(
        input,
        weights.get(&format!("{prefix}.ffn_gate.weight"))?,
        None,
    )?;
    if gate.shape.last() != intermediate_size as i64 {
        bail!("layer {layer} feed-forward width does not match Gemma 4 config");
    }
    let up = graph.linear(
        input,
        weights.get(&format!("{prefix}.ffn_up.weight"))?,
        None,
    )?;
    graph.linear(
        graph.geglu(gate, up)?,
        weights.get(&format!("{prefix}.ffn_down.weight"))?,
        None,
    )
}

fn validate_config(config: &Config) -> Result<()> {
    let text = &config.text_config;
    let valid_layer_types = text
        .layer_types
        .iter()
        .all(|layer_type| layer_type == SLIDING_ATTENTION || layer_type == FULL_ATTENTION);
    if config.model_type != "gemma4"
        || text.num_hidden_layers != 35
        || text.layer_types.len() != text.num_hidden_layers
        || text.num_attention_heads % text.num_key_value_heads != 0
        || text.num_kv_shared_layers < 2
        || text.num_kv_shared_layers + 2 > text.num_hidden_layers
        || text.hidden_size_per_layer_input == 0
        || text.enable_moe_block
        || !valid_layer_types
    {
        bail!("unsupported or inconsistent Gemma 4 E2B configuration");
    }
    let first_shared_layer = text.num_hidden_layers - text.num_kv_shared_layers;
    if text.layer_types[first_shared_layer - 2] != SLIDING_ATTENTION
        || text.layer_types[first_shared_layer - 1] != FULL_ATTENTION
    {
        bail!("Gemma 4 shared-KV source layers are inconsistent");
    }
    Ok(())
}

fn head_dim(config: &TextConfig, layer_type: &str) -> usize {
    if layer_type == SLIDING_ATTENTION {
        config.head_dim
    } else {
        config.global_head_dim
    }
}

fn weighted_rms_norm(
    graph: &GraphBuilder,
    value: Tensor,
    weight: Tensor,
    epsilon: f32,
) -> Result<Tensor> {
    graph.mul(graph.rms_norm(value, epsilon)?, weight)
}

fn apply_rope(
    graph: &GraphBuilder,
    value: Tensor,
    positions: Tensor,
    head_dim: usize,
    config: &RopeConfig,
    original_context: usize,
    frequency_factors: Option<Tensor>,
) -> Result<Tensor> {
    let rotated_dimensions = (head_dim as f32 * config.partial_rotary_factor) as usize;
    if rotated_dimensions == head_dim || frequency_factors.is_some() {
        return graph.rope_neox(
            value,
            positions,
            head_dim,
            config.rope_theta,
            original_context,
            frequency_factors,
        );
    }

    let rotated_half = (rotated_dimensions / 2) as i64;
    let head_half = (head_dim / 2) as i64;
    let first = graph.slice(value, 3, 0, rotated_half)?;
    let second = graph.slice(value, 3, head_half, rotated_half)?;
    let selected = graph.concat(first, second, 3)?;
    let rotated = graph.rope_neox(
        selected,
        positions,
        rotated_dimensions,
        config
            .rope_theta
            .powf(rotated_dimensions as f32 / head_dim as f32),
        original_context,
        None,
    )?;
    let first = graph.slice(rotated, 3, 0, rotated_half)?;
    let second = graph.slice(rotated, 3, rotated_half, rotated_half)?;
    let first_unrotated = graph.slice(value, 3, rotated_half, head_half - rotated_half)?;
    let second_unrotated =
        graph.slice(value, 3, head_half + rotated_half, head_half - rotated_half)?;
    let first = graph.concat(first, first_unrotated, 3)?;
    let second = graph.concat(second, second_unrotated, 3)?;
    graph.concat(first, second, 3)
}

fn attention_mask_f16(
    query_positions: &[i32],
    capacity: usize,
    sliding_window: Option<usize>,
) -> Vec<u16> {
    attention_mask(query_positions, capacity, sliding_window)
        .into_iter()
        .map(|value| if value == 0.0 { 0 } else { 0xfc00 })
        .collect()
}

fn attention_mask(
    query_positions: &[i32],
    capacity: usize,
    sliding_window: Option<usize>,
) -> Vec<f32> {
    let mut mask = Vec::with_capacity(query_positions.len() * capacity);
    for query in query_positions {
        for key in 0..capacity {
            let query = *query as usize;
            let causal = key <= query;
            let within_window = sliding_window.is_none_or(|window| query - key.min(query) < window);
            mask.push(if causal && within_window { 0.0 } else { -1.0e9 });
        }
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::{attention_mask, attention_mask_f16};

    #[test]
    fn causal_mask_blocks_future_tokens() {
        let blocked = -1.0e9;
        assert_eq!(
            attention_mask(&[0, 1, 2], 3, None),
            vec![0.0, blocked, blocked, 0.0, 0.0, blocked, 0.0, 0.0, 0.0,]
        );
    }

    #[test]
    fn flash_attention_mask_uses_negative_infinity() {
        assert_eq!(attention_mask_f16(&[0], 2, None), vec![0, 0xfc00]);
    }

    #[test]
    fn sliding_mask_blocks_old_tokens() {
        let blocked = -1.0e9;
        assert_eq!(
            attention_mask(&[0, 1, 2, 3], 4, Some(2)),
            vec![
                0.0, blocked, blocked, blocked, 0.0, 0.0, blocked, blocked, blocked, 0.0, 0.0,
                blocked, blocked, blocked, 0.0, 0.0,
            ]
        );
    }
}
