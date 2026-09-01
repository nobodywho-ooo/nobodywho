use anyhow::{bail, Context, Result};
use ggml_runtime::{Backend, BackendKind, GraphBuilder, Shape, Tensor, Weights};
use serde::Deserialize;
use std::path::Path;
use std::time::Instant;

const GRAPH_METADATA_BYTES: usize = 512 * 1024 * 1024;
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

struct KeyValues {
    key: Vec<f32>,
    value: Vec<f32>,
}

pub struct Model {
    backend: Backend,
    weights: Weights,
    config: Config,
}

impl Model {
    pub fn load(model_dir: &Path, backend_name: &str, threads: usize) -> Result<Self> {
        let config: Config =
            serde_json::from_reader(std::fs::File::open(model_dir.join("config.json"))?)?;
        validate_config(&config)?;
        let backend_kind = match backend_name.to_ascii_lowercase().as_str() {
            "cpu" => BackendKind::Cpu,
            "metal" => BackendKind::Metal,
            value => bail!("unsupported backend {value}; choose cpu or metal"),
        };
        let backend = Backend::new(backend_kind)?;
        backend.set_threads(threads);
        let started = Instant::now();
        let weights = Weights::load(&model_dir.join("gemma-4-E2B-it-Q4_K_M.gguf"), &backend)?;
        println!(
            "Loaded {} Gemma 4 tensors on {} in {:.2}s",
            weights.len(),
            backend_kind.name(),
            started.elapsed().as_secs_f32()
        );
        Ok(Self {
            backend,
            weights,
            config,
        })
    }

    pub fn max_context(&self) -> usize {
        self.config.text_config.max_position_embeddings
    }

    pub fn logits(&self, token_ids: &[u32]) -> Result<Vec<f32>> {
        let config = &self.config.text_config;
        if token_ids.is_empty() || token_ids.len() > config.max_position_embeddings {
            bail!("invalid token count");
        }
        let token_ids = token_ids
            .iter()
            .map(|token| {
                if *token as usize >= config.vocab_size {
                    bail!("token ID {token} exceeds vocabulary size");
                }
                i32::try_from(*token).context("token ID exceeds i32")
            })
            .collect::<Result<Vec<_>>>()?;
        let (mut hidden, per_layer) = self.embed(&token_ids)?;
        let sequence_length = token_ids.len();
        let first_shared_layer = config.num_hidden_layers - config.num_kv_shared_layers;
        let sliding_source_layer = first_shared_layer - 2;
        let full_source_layer = first_shared_layer - 1;
        let mut sliding_key_values = None;
        let mut full_key_values = None;

        for layer in 0..config.num_hidden_layers {
            let layer_type = &config.layer_types[layer];
            let shared = if layer < first_shared_layer {
                None
            } else if layer_type == SLIDING_ATTENTION {
                sliding_key_values.as_ref()
            } else {
                full_key_values.as_ref()
            };
            let per_layer_start = layer * sequence_length * config.hidden_size_per_layer_input;
            let per_layer_end =
                per_layer_start + sequence_length * config.hidden_size_per_layer_input;
            let capture_key_values = layer == sliding_source_layer || layer == full_source_layer;
            let (next_hidden, key_values) = self.run_layer(
                layer,
                &hidden,
                &per_layer[per_layer_start..per_layer_end],
                shared,
                capture_key_values,
                sequence_length,
            )?;
            hidden = next_hidden;
            if layer == sliding_source_layer {
                sliding_key_values = key_values;
            } else if layer == full_source_layer {
                full_key_values = key_values;
            }
        }
        self.run_head(&hidden[(sequence_length - 1) * config.hidden_size..])
    }

    fn embed(&self, token_ids: &[i32]) -> Result<(Vec<f32>, Vec<f32>)> {
        let config = &self.config.text_config;
        let graph = GraphBuilder::new(GRAPH_METADATA_BYTES)?;
        let ids = graph.input_i32(&[1, token_ids.len() as i64])?;
        let hidden = graph.scale(
            graph.embedding(ids, self.weights.get("token_embd.weight")?)?,
            (config.hidden_size as f32).sqrt(),
        )?;

        let per_layer_tokens = graph.scale(
            graph.embedding(ids, self.weights.get("per_layer_token_embd.weight")?)?,
            (config.hidden_size_per_layer_input as f32).sqrt(),
        )?;
        let per_layer_tokens = graph.reshape(
            per_layer_tokens,
            &[
                1,
                token_ids.len() as i64,
                config.num_hidden_layers as i64,
                config.hidden_size_per_layer_input as i64,
            ],
        )?;
        let projected = graph.scale(
            graph.linear(
                hidden,
                self.weights.get("per_layer_model_proj.weight")?,
                None,
            )?,
            1.0 / (config.hidden_size as f32).sqrt(),
        )?;
        let projected = graph.reshape(
            projected,
            &[
                1,
                token_ids.len() as i64,
                config.num_hidden_layers as i64,
                config.hidden_size_per_layer_input as i64,
            ],
        )?;
        let projected = weighted_rms_norm(
            &graph,
            projected,
            self.weights.get("per_layer_proj_norm.weight")?,
            config.rms_norm_eps,
        )?;
        let per_layer = graph.scale(
            graph.add(projected, per_layer_tokens)?,
            1.0 / 2.0_f32.sqrt(),
        )?;
        let per_layer = graph.contiguous(graph.transpose(per_layer, &[0, 2, 1, 3])?)?;

        let hidden_elements = hidden.shape.elements();
        let output = flatten_concat(&graph, &[hidden, per_layer])?;
        let computation = graph.finish(output, &self.backend)?;
        computation.set_i32(ids, token_ids)?;
        computation.compute(&self.backend)?;
        let values = computation.output_f32()?;
        Ok((
            values[..hidden_elements].to_vec(),
            values[hidden_elements..].to_vec(),
        ))
    }

    fn run_layer(
        &self,
        layer: usize,
        hidden: &[f32],
        per_layer: &[f32],
        shared_key_values: Option<&KeyValues>,
        capture_key_values: bool,
        sequence_length: usize,
    ) -> Result<(Vec<f32>, Option<KeyValues>)> {
        let config = &self.config.text_config;
        let layer_type = &config.layer_types[layer];
        let prefix = format!("blk.{layer}");
        let graph = GraphBuilder::new(GRAPH_METADATA_BYTES)?;
        let input = graph.input_f32(&[1, sequence_length as i64, config.hidden_size as i64])?;
        let per_layer_input = graph.input_f32(&[
            1,
            sequence_length as i64,
            config.hidden_size_per_layer_input as i64,
        ])?;
        let normalized = weighted_rms_norm(
            &graph,
            input,
            self.weights.get(&format!("{prefix}.attn_norm.weight"))?,
            config.rms_norm_eps,
        )?;
        let (attention, key, value, shared_inputs) = self.attention(
            &graph,
            normalized,
            &prefix,
            layer_type,
            shared_key_values,
            sequence_length,
        )?;
        let attention = weighted_rms_norm(
            &graph,
            attention,
            self.weights
                .get(&format!("{prefix}.post_attention_norm.weight"))?,
            config.rms_norm_eps,
        )?;
        let attention_output = graph.add(input, attention)?;
        let normalized = weighted_rms_norm(
            &graph,
            attention_output,
            self.weights.get(&format!("{prefix}.ffn_norm.weight"))?,
            config.rms_norm_eps,
        )?;
        let feed_forward = self.feed_forward(&graph, normalized, &prefix, layer)?;
        let feed_forward = weighted_rms_norm(
            &graph,
            feed_forward,
            self.weights
                .get(&format!("{prefix}.post_ffw_norm.weight"))?,
            config.rms_norm_eps,
        )?;
        let mut output = graph.add(attention_output, feed_forward)?;

        let gate = graph.gelu_tanh(graph.linear(
            output,
            self.weights.get(&format!("{prefix}.inp_gate.weight"))?,
            None,
        )?)?;
        let projected = graph.linear(
            graph.mul(gate, per_layer_input)?,
            self.weights.get(&format!("{prefix}.proj.weight"))?,
            None,
        )?;
        let projected = weighted_rms_norm(
            &graph,
            projected,
            self.weights.get(&format!("{prefix}.post_norm.weight"))?,
            config.rms_norm_eps,
        )?;
        output = graph.add(output, projected)?;
        output = graph.mul(
            output,
            self.weights
                .get(&format!("{prefix}.layer_output_scale.weight"))?,
        )?;

        let hidden_elements = output.shape.elements();
        let key_elements = key.shape.elements();
        let output_tensor = if capture_key_values {
            flatten_concat(&graph, &[output, key, value])?
        } else {
            graph.reshape(graph.contiguous(output)?, &[hidden_elements as i64])?
        };
        let computation = graph.finish(output_tensor, &self.backend)?;
        computation.set_f32(input, hidden)?;
        computation.set_f32(per_layer_input, per_layer)?;
        if let (Some(shared), Some((key_input, value_input))) = (shared_key_values, shared_inputs) {
            computation.set_f32(key_input, &shared.key)?;
            computation.set_f32(value_input, &shared.value)?;
        }
        computation.compute(&self.backend)?;
        let values = computation.output_f32()?;
        let next_hidden = values[..hidden_elements].to_vec();
        let key_values = capture_key_values.then(|| KeyValues {
            key: values[hidden_elements..hidden_elements + key_elements].to_vec(),
            value: values[hidden_elements + key_elements..].to_vec(),
        });
        Ok((next_hidden, key_values))
    }

    fn attention(
        &self,
        graph: &GraphBuilder,
        input: Tensor,
        prefix: &str,
        layer_type: &str,
        shared_key_values: Option<&KeyValues>,
        sequence_length: usize,
    ) -> Result<(Tensor, Tensor, Tensor, Option<(Tensor, Tensor)>)> {
        let config = &self.config.text_config;
        let head_dim = self.head_dim(layer_type);
        let rope_config = if layer_type == SLIDING_ATTENTION {
            &config.rope_parameters.sliding_attention
        } else {
            &config.rope_parameters.full_attention
        };
        let rope = Rope::new(
            sequence_length,
            head_dim,
            rope_config.rope_theta,
            rope_config.partial_rotary_factor,
        );
        let cosine = graph.constant_f32(
            &[1, sequence_length as i64, 1, head_dim as i64],
            rope.cosine,
        )?;
        let sine =
            graph.constant_f32(&[1, sequence_length as i64, 1, head_dim as i64], rope.sine)?;
        let query = graph.linear(
            input,
            self.weights.get(&format!("{prefix}.attn_q.weight"))?,
            None,
        )?;
        let query = graph.reshape(
            graph.contiguous(query)?,
            &[
                1,
                sequence_length as i64,
                config.num_attention_heads as i64,
                head_dim as i64,
            ],
        )?;
        let query = weighted_rms_norm(
            graph,
            query,
            self.weights.get(&format!("{prefix}.attn_q_norm.weight"))?,
            config.rms_norm_eps,
        )?;
        let query = apply_rope(graph, query, cosine, sine, head_dim)?;
        let query = graph.transpose(query, &[0, 2, 1, 3])?;

        let (key, value, shared_inputs) = if shared_key_values.is_some() {
            let shape = [1, 1, sequence_length as i64, head_dim as i64];
            let key = graph.input_f32(&shape)?;
            let value = graph.input_f32(&shape)?;
            (key, value, Some((key, value)))
        } else {
            let key = graph.linear(
                input,
                self.weights.get(&format!("{prefix}.attn_k.weight"))?,
                None,
            )?;
            let value = graph.linear(
                input,
                self.weights.get(&format!("{prefix}.attn_v.weight"))?,
                None,
            )?;
            let shape = [
                1,
                sequence_length as i64,
                config.num_key_value_heads as i64,
                head_dim as i64,
            ];
            let key = graph.reshape(graph.contiguous(key)?, &shape)?;
            let value = graph.reshape(graph.contiguous(value)?, &shape)?;
            let key = weighted_rms_norm(
                graph,
                key,
                self.weights.get(&format!("{prefix}.attn_k_norm.weight"))?,
                config.rms_norm_eps,
            )?;
            let value = graph.rms_norm(value, config.rms_norm_eps)?;
            let key = graph.transpose(
                apply_rope(graph, key, cosine, sine, head_dim)?,
                &[0, 2, 1, 3],
            )?;
            let value = graph.transpose(value, &[0, 2, 1, 3])?;
            (key, value, None)
        };

        let attention_shape = Shape::new(&[
            1,
            config.num_attention_heads as i64,
            sequence_length as i64,
            head_dim as i64,
        ])?;
        let key_for_attention = graph.broadcast(key, attention_shape)?;
        let value_for_attention = graph.broadcast(value, attention_shape)?;
        let key_for_attention = graph.transpose(key_for_attention, &[0, 1, 3, 2])?;
        let scores = graph.matmul(query, key_for_attention)?;
        let mask = graph.constant_f32(
            &[1, 1, sequence_length as i64, sequence_length as i64],
            attention_mask(
                sequence_length,
                (layer_type == SLIDING_ATTENTION).then_some(config.sliding_window),
            ),
        )?;
        let probabilities = graph.softmax(graph.add(scores, mask)?)?;
        let attended = graph.matmul(probabilities, value_for_attention)?;
        let attended = graph.transpose(attended, &[0, 2, 1, 3])?;
        let attended = graph.reshape(
            graph.contiguous(attended)?,
            &[
                1,
                sequence_length as i64,
                (config.num_attention_heads * head_dim) as i64,
            ],
        )?;
        let output = graph.linear(
            attended,
            self.weights.get(&format!("{prefix}.attn_output.weight"))?,
            None,
        )?;
        Ok((output, key, value, shared_inputs))
    }

    fn feed_forward(
        &self,
        graph: &GraphBuilder,
        input: Tensor,
        prefix: &str,
        layer: usize,
    ) -> Result<Tensor> {
        let config = &self.config.text_config;
        let first_shared_layer = config.num_hidden_layers - config.num_kv_shared_layers;
        let intermediate_size = if config.use_double_wide_mlp && layer >= first_shared_layer {
            config.intermediate_size * 2
        } else {
            config.intermediate_size
        };
        let gate = graph.linear(
            input,
            self.weights.get(&format!("{prefix}.ffn_gate.weight"))?,
            None,
        )?;
        if gate.shape.last() != intermediate_size as i64 {
            bail!("layer {layer} feed-forward width does not match Gemma 4 config");
        }
        let up = graph.linear(
            input,
            self.weights.get(&format!("{prefix}.ffn_up.weight"))?,
            None,
        )?;
        graph.linear(
            graph.mul(graph.gelu_tanh(gate)?, up)?,
            self.weights.get(&format!("{prefix}.ffn_down.weight"))?,
            None,
        )
    }

    fn run_head(&self, hidden: &[f32]) -> Result<Vec<f32>> {
        let config = &self.config.text_config;
        let graph = GraphBuilder::new(GRAPH_METADATA_BYTES)?;
        let input = graph.input_f32(&[1, 1, config.hidden_size as i64])?;
        let normalized = weighted_rms_norm(
            &graph,
            input,
            self.weights.get("output_norm.weight")?,
            config.rms_norm_eps,
        )?;
        let logits = graph.linear(normalized, self.weights.get("token_embd.weight")?, None)?;
        let logits = graph.scale(
            graph.tanh(graph.scale(logits, 1.0 / config.final_logit_softcapping)?)?,
            config.final_logit_softcapping,
        )?;
        let logits = graph.contiguous(logits)?;
        let computation = graph.finish(logits, &self.backend)?;
        computation.set_f32(input, hidden)?;
        computation.compute(&self.backend)?;
        let logits = computation.output_f32()?;
        if logits.len() != config.vocab_size {
            bail!("language-model head returned {} logits", logits.len());
        }
        Ok(logits)
    }

    fn head_dim(&self, layer_type: &str) -> usize {
        if layer_type == SLIDING_ATTENTION {
            self.config.text_config.head_dim
        } else {
            self.config.text_config.global_head_dim
        }
    }
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

fn weighted_rms_norm(
    graph: &GraphBuilder,
    value: Tensor,
    weight: Tensor,
    epsilon: f32,
) -> Result<Tensor> {
    graph.mul(graph.rms_norm(value, epsilon)?, weight)
}

fn flatten_concat(graph: &GraphBuilder, values: &[Tensor]) -> Result<Tensor> {
    let mut values = values.iter();
    let first = values.next().context("cannot concatenate no tensors")?;
    let mut output = graph.reshape(graph.contiguous(*first)?, &[first.shape.elements() as i64])?;
    for value in values {
        let value = graph.reshape(graph.contiguous(*value)?, &[value.shape.elements() as i64])?;
        output = graph.concat(output, value, 0)?;
    }
    Ok(output)
}

struct Rope {
    cosine: Vec<f32>,
    sine: Vec<f32>,
}

impl Rope {
    fn new(
        sequence_length: usize,
        head_dim: usize,
        theta: f32,
        partial_rotary_factor: f32,
    ) -> Self {
        let rotated_pairs = (head_dim as f32 * partial_rotary_factor) as usize / 2;
        let mut cosine = Vec::with_capacity(sequence_length * head_dim);
        let mut sine = Vec::with_capacity(sequence_length * head_dim);
        for position in 0..sequence_length {
            for _ in 0..2 {
                for pair in 0..head_dim / 2 {
                    let frequency = if pair < rotated_pairs {
                        position as f32 / theta.powf((pair * 2) as f32 / head_dim as f32)
                    } else {
                        0.0
                    };
                    cosine.push(frequency.cos());
                    sine.push(frequency.sin());
                }
            }
        }
        Self { cosine, sine }
    }
}

fn apply_rope(
    graph: &GraphBuilder,
    input: Tensor,
    cosine: Tensor,
    sine: Tensor,
    head_dim: usize,
) -> Result<Tensor> {
    let half = (head_dim / 2) as i64;
    let first = graph.slice(input, 3, 0, half)?;
    let second = graph.slice(input, 3, half, half)?;
    let rotated = graph.concat(graph.scale(second, -1.0)?, first, 3)?;
    graph.add(graph.mul(input, cosine)?, graph.mul(rotated, sine)?)
}

fn attention_mask(sequence_length: usize, sliding_window: Option<usize>) -> Vec<f32> {
    let mut mask = Vec::with_capacity(sequence_length * sequence_length);
    for query in 0..sequence_length {
        for key in 0..sequence_length {
            let causal = key <= query;
            let within_window = sliding_window.is_none_or(|window| query - key.min(query) < window);
            mask.push(if causal && within_window { 0.0 } else { -1.0e9 });
        }
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::{attention_mask, Rope};

    #[test]
    fn causal_mask_blocks_future_tokens() {
        let blocked = -1.0e9;
        assert_eq!(
            attention_mask(3, None),
            vec![0.0, blocked, blocked, 0.0, 0.0, blocked, 0.0, 0.0, 0.0]
        );
    }

    #[test]
    fn sliding_mask_blocks_old_tokens() {
        let blocked = -1.0e9;
        assert_eq!(
            attention_mask(4, Some(2)),
            vec![
                0.0, blocked, blocked, blocked, 0.0, 0.0, blocked, blocked, blocked, 0.0, 0.0,
                blocked, blocked, blocked, 0.0, 0.0,
            ]
        );
    }

    #[test]
    fn proportional_rope_leaves_unrotated_dimensions_unchanged() {
        let rope = Rope::new(2, 8, 10_000.0, 0.25);
        for index in [9, 10, 11, 13, 14, 15] {
            assert_eq!(rope.cosine[index], 1.0);
            assert_eq!(rope.sine[index], 0.0);
        }
    }
}
