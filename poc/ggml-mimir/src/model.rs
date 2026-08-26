use anyhow::{bail, Context, Result};
use ggml_runtime::{Backend, BackendKind, GraphBuilder, Tensor, Weights};
use serde::Deserialize;
use std::path::Path;
use std::time::Instant;

const EMBEDDING_METADATA_BYTES: usize = 128 * 1024 * 1024;
const STACK_METADATA_BYTES: usize = 512 * 1024 * 1024;
const HEAD_METADATA_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct Config {
    model_type: String,
    vocab_size: usize,
    hidden_size: usize,
    intermediate_size: usize,
    num_hidden_layers: usize,
    num_attention_heads: usize,
    num_key_value_heads: usize,
    head_dim: usize,
    #[serde(rename = "H_cycles")]
    high_cycles: usize,
    #[serde(rename = "L_cycles")]
    low_cycles: usize,
    max_position_embeddings: usize,
    rms_norm_eps: f32,
    rope_theta: f32,
    embedding_scale: f32,
    eos_token_id: u32,
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
        if config.model_type != "hrm_text"
            || config.num_attention_heads != config.num_key_value_heads
            || config.hidden_size != config.num_attention_heads * config.head_dim
            || config.num_hidden_layers == 0
        {
            bail!("unsupported or inconsistent HRM-Text configuration");
        }
        let backend_kind = match backend_name.to_ascii_lowercase().as_str() {
            "cpu" => BackendKind::Cpu,
            "metal" => BackendKind::Metal,
            value => bail!("unsupported backend {value}; choose cpu or metal"),
        };
        let backend = Backend::new(backend_kind)?;
        backend.set_threads(threads);
        let started = Instant::now();
        let weights = Weights::load_safetensors(&model_dir.join("model.safetensors"), &backend)?;
        println!(
            "Loaded {} Mimir tensors on {} in {:.2}s",
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

    pub fn eos_token_id(&self) -> u32 {
        self.config.eos_token_id
    }

    pub fn max_context(&self) -> usize {
        self.config.max_position_embeddings
    }

    pub fn logits(&self, token_ids: &[u32], prefix_length: usize) -> Result<Vec<f32>> {
        if token_ids.is_empty()
            || token_ids.len() > self.config.max_position_embeddings
            || prefix_length == 0
            || prefix_length > token_ids.len()
        {
            bail!("invalid token or prefix length");
        }
        let token_ids_i32 = token_ids
            .iter()
            .map(|token| {
                if *token as usize >= self.config.vocab_size {
                    bail!("token ID {token} exceeds vocabulary size");
                }
                i32::try_from(*token).context("token ID exceeds i32")
            })
            .collect::<Result<Vec<_>>>()?;
        let mut high = self.embed(&token_ids_i32)?;
        let mut low = self.initial_low_state(token_ids.len())?;
        let rope = Rope::new(
            token_ids.len(),
            self.config.head_dim,
            self.config.rope_theta,
        );
        let mask = prefix_mask(token_ids.len(), prefix_length);

        for _ in 0..self.config.high_cycles {
            for _ in 0..self.config.low_cycles {
                add_in_place(&mut low, &high)?;
                low = self.run_stack("L_module", &low, token_ids.len(), &rope, &mask)?;
            }
            add_in_place(&mut high, &low)?;
            high = self.run_stack("H_module", &high, token_ids.len(), &rope, &mask)?;
        }
        self.run_head(&high[(token_ids.len() - 1) * self.config.hidden_size..])
    }

    fn embed(&self, token_ids: &[i32]) -> Result<Vec<f32>> {
        let graph_builder = GraphBuilder::new(EMBEDDING_METADATA_BYTES)?;
        let ids = graph_builder.input_i32(&[1, token_ids.len() as i64])?;
        let table = self.weights.get("model.embed_tokens.weight")?;
        let output = graph_builder.scale(
            graph_builder.embedding(ids, table)?,
            self.config.embedding_scale,
        )?;
        let graph = graph_builder.finish(output, &self.backend)?;
        graph.set_i32(ids, token_ids)?;
        graph.compute(&self.backend)?;
        graph.output_f32()
    }

    fn initial_low_state(&self, sequence_length: usize) -> Result<Vec<f32>> {
        let graph_builder = GraphBuilder::new(EMBEDDING_METADATA_BYTES)?;
        let initial = graph_builder.cast_f32(self.weights.get("model.z_L_init")?)?;
        let initial = graph_builder.broadcast(
            initial,
            ggml_runtime::Shape::new(&[1, sequence_length as i64, self.config.hidden_size as i64])?,
        )?;
        let output = graph_builder.contiguous(initial)?;
        let graph = graph_builder.finish(output, &self.backend)?;
        graph.compute(&self.backend)?;
        graph.output_f32()
    }

    fn run_stack(
        &self,
        stack: &str,
        input: &[f32],
        sequence_length: usize,
        rope: &Rope,
        mask: &[f32],
    ) -> Result<Vec<f32>> {
        let graph_builder = GraphBuilder::new(STACK_METADATA_BYTES)?;
        let input_tensor = graph_builder.input_f32(&[
            1,
            sequence_length as i64,
            self.config.hidden_size as i64,
        ])?;
        let cosine = graph_builder.constant_f32(
            &[1, sequence_length as i64, 1, self.config.head_dim as i64],
            rope.cosine.clone(),
        )?;
        let sine = graph_builder.constant_f32(
            &[1, sequence_length as i64, 1, self.config.head_dim as i64],
            rope.sine.clone(),
        )?;
        let attention_mask = graph_builder.constant_f32(
            &[1, 1, sequence_length as i64, sequence_length as i64],
            mask.to_vec(),
        )?;
        let mut hidden = input_tensor;
        for layer in 0..self.config.num_hidden_layers {
            let prefix = format!("model.{stack}.layers.{layer}");
            let normalized = graph_builder.rms_norm(hidden, self.config.rms_norm_eps)?;
            let attention = self.attention(
                &graph_builder,
                normalized,
                cosine,
                sine,
                attention_mask,
                &prefix,
            )?;
            hidden = graph_builder.add(hidden, attention)?;
            let normalized = graph_builder.rms_norm(hidden, self.config.rms_norm_eps)?;
            let mlp = self.mlp(&graph_builder, normalized, &prefix)?;
            hidden = graph_builder.add(hidden, mlp)?;
        }
        hidden = graph_builder.rms_norm(hidden, self.config.rms_norm_eps)?;
        let output = graph_builder.contiguous(hidden)?;
        let graph = graph_builder.finish(output, &self.backend)?;
        graph.set_f32(input_tensor, input)?;
        graph.compute(&self.backend)?;
        graph.output_f32()
    }

    fn attention(
        &self,
        graph: &GraphBuilder,
        input: Tensor,
        cosine: Tensor,
        sine: Tensor,
        mask: Tensor,
        prefix: &str,
    ) -> Result<Tensor> {
        let projected = graph.linear(
            input,
            self.weights
                .get(&format!("{prefix}.attn.gqkv_proj.weight"))?,
            None,
        )?;
        let projected = graph.reshape(
            graph.contiguous(projected)?,
            &[
                1,
                input.shape.at(1),
                (self.config.num_attention_heads * 4) as i64,
                self.config.head_dim as i64,
            ],
        )?;
        let heads = self.config.num_attention_heads as i64;
        let gate = graph.slice(projected, 2, 0, heads)?;
        let query = graph.slice(projected, 2, heads, heads)?;
        let key = graph.slice(projected, 2, heads * 2, heads)?;
        let value = graph.slice(projected, 2, heads * 3, heads)?;
        let query = apply_rope(graph, query, cosine, sine, self.config.head_dim)?;
        let key = apply_rope(graph, key, cosine, sine, self.config.head_dim)?;
        let query = graph.transpose(query, &[0, 2, 1, 3])?;
        let key = graph.transpose(key, &[0, 2, 3, 1])?;
        let mut scores = graph.matmul(query, key)?;
        scores = graph.scale(scores, 1.0 / (self.config.head_dim as f32).sqrt())?;
        scores = graph.add(scores, mask)?;
        let probabilities = graph.softmax(scores)?;
        let value = graph.transpose(value, &[0, 2, 1, 3])?;
        let attended = graph.matmul(probabilities, value)?;
        let attended = graph.transpose(attended, &[0, 2, 1, 3])?;
        let attended = graph.mul(graph.sigmoid(gate)?, attended)?;
        let attended = graph.reshape(
            graph.contiguous(attended)?,
            &[1, input.shape.at(1), self.config.hidden_size as i64],
        )?;
        graph.linear(
            attended,
            self.weights.get(&format!("{prefix}.attn.o_proj.weight"))?,
            None,
        )
    }

    fn mlp(&self, graph: &GraphBuilder, input: Tensor, prefix: &str) -> Result<Tensor> {
        let projected = graph.linear(
            input,
            self.weights
                .get(&format!("{prefix}.mlp.gate_up_proj.weight"))?,
            None,
        )?;
        let gate = graph.slice(projected, 2, 0, self.config.intermediate_size as i64)?;
        let up = graph.slice(
            projected,
            2,
            self.config.intermediate_size as i64,
            self.config.intermediate_size as i64,
        )?;
        let activated = graph.mul(graph.silu(gate)?, up)?;
        graph.linear(
            activated,
            self.weights
                .get(&format!("{prefix}.mlp.down_proj.weight"))?,
            None,
        )
    }

    fn run_head(&self, hidden: &[f32]) -> Result<Vec<f32>> {
        let graph_builder = GraphBuilder::new(HEAD_METADATA_BYTES)?;
        let input = graph_builder.input_f32(&[1, 1, self.config.hidden_size as i64])?;
        let output = graph_builder.linear(input, self.weights.get("lm_head.weight")?, None)?;
        let output = graph_builder.contiguous(output)?;
        let graph = graph_builder.finish(output, &self.backend)?;
        graph.set_f32(input, hidden)?;
        graph.compute(&self.backend)?;
        let logits = graph.output_f32()?;
        if logits.len() != self.config.vocab_size {
            bail!("language-model head returned {} logits", logits.len());
        }
        Ok(logits)
    }
}

struct Rope {
    cosine: Vec<f32>,
    sine: Vec<f32>,
}

impl Rope {
    fn new(sequence_length: usize, head_dim: usize, theta: f32) -> Self {
        let mut cosine = Vec::with_capacity(sequence_length * head_dim);
        let mut sine = Vec::with_capacity(sequence_length * head_dim);
        for position in 0..sequence_length {
            let frequencies = (0..head_dim / 2)
                .map(|index| position as f32 / theta.powf((index * 2) as f32 / head_dim as f32));
            let frequencies = frequencies.collect::<Vec<_>>();
            for frequency in frequencies.iter().chain(frequencies.iter()) {
                cosine.push(frequency.cos());
                sine.push(frequency.sin());
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

fn prefix_mask(sequence_length: usize, prefix_length: usize) -> Vec<f32> {
    let mut mask = Vec::with_capacity(sequence_length * sequence_length);
    for query in 0..sequence_length {
        for key in 0..sequence_length {
            let allowed = if query < prefix_length {
                key < prefix_length
            } else {
                key <= query
            };
            mask.push(if allowed { 0.0 } else { -1.0e9 });
        }
    }
    mask
}

fn add_in_place(left: &mut [f32], right: &[f32]) -> Result<()> {
    if left.len() != right.len() {
        bail!("hidden-state size mismatch");
    }
    for (left, right) in left.iter_mut().zip(right) {
        *left += right;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::prefix_mask;

    #[test]
    fn prefix_tokens_are_bidirectional_and_response_is_causal() {
        let blocked = -1.0e9;
        assert_eq!(
            prefix_mask(4, 2),
            vec![
                0.0, 0.0, blocked, blocked, 0.0, 0.0, blocked, blocked, 0.0, 0.0, 0.0, blocked,
                0.0, 0.0, 0.0, 0.0,
            ]
        );
    }
}
