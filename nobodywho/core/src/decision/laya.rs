use super::{DecisionQuestion, DecisionRequest, DecisionResponse, DecisionUsage, PreparedQuestion};
use crate::errors::{DecisionError, ReadError};
use crate::inference::InferenceEngine;
use crate::llm::Model;
use crate::template::MINIJINJA_ENV;
use crate::tokenizer::{TokenizerChunk, TokenizerChunks};
use indexmap::IndexMap;
use llama_cpp_2::context::params::LlamaPoolingType;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::token::LlamaToken;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::{Arc, LazyLock};
use yoke::Yoke;

// llama.cpp's decision templates use Python-style JSON separators and do not
// HTML-escape '<', '>' or '&'. MiniJinja's built-in tojson does both differently.
static LAYA_ENV: LazyLock<minijinja::Environment<'static>> = LazyLock::new(|| {
    let mut environment = MINIJINJA_ENV.clone();
    environment.add_filter("tojson", decision_to_json);
    environment
});

struct DecisionJsonFormatter;

impl serde_json::ser::Formatter for DecisionJsonFormatter {
    fn begin_array_value<W: Write + ?Sized>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        if !first {
            writer.write_all(b", ")?;
        }
        Ok(())
    }

    fn begin_object_key<W: Write + ?Sized>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        self.begin_array_value(writer, first)
    }

    fn begin_object_value<W: Write + ?Sized>(&mut self, writer: &mut W) -> io::Result<()> {
        writer.write_all(b": ")
    }
}

fn decision_to_json(value: minijinja::Value) -> Result<String, minijinja::Error> {
    let mut bytes = Vec::new();
    let mut serializer = serde_json::Serializer::with_formatter(&mut bytes, DecisionJsonFormatter);
    value.serialize(&mut serializer).map_err(|error| {
        minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, error.to_string())
    })?;
    String::from_utf8(bytes).map_err(|error| {
        minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, error.to_string())
    })
}

impl PreparedQuestion<'_> {
    fn column(&self) -> usize {
        match self.question {
            DecisionQuestion::Choice { .. } => 0,
            DecisionQuestion::Score { .. } => 1,
            DecisionQuestion::Boolean { .. } => 2,
        }
    }
}

pub(super) struct Laya {
    engine: Yoke<InferenceEngine<'static>, Arc<Model>>,
    laya: LayaState,
}

impl Laya {
    pub(super) fn new(model: Arc<Model>, n_ctx: u32) -> Result<Self, DecisionError> {
        if n_ctx == 0 {
            return Err(DecisionError::InvalidRequest(
                "n_ctx must be positive".into(),
            ));
        }
        let laya = LayaState::new(&model.language_model)?;
        let engine = Yoke::try_attach_to_cart(model, |model| {
            InferenceEngine::new_with_type(model, n_ctx, true, None, None, LlamaPoolingType::None)
        })?;
        Ok(Self { engine, laya })
    }

    pub(super) fn decide(
        &mut self,
        request: DecisionRequest,
    ) -> Result<DecisionResponse, DecisionError> {
        if request.state.is_null() || request.questions.is_empty() {
            return Err(DecisionError::InvalidRequest(
                "state must not be null and questions must not be empty".into(),
            ));
        }
        let prepared = request
            .questions
            .iter()
            .map(|(id, question)| PreparedQuestion::new(id, question))
            .collect::<Result<Vec<_>, _>>()?;
        let mut answers = IndexMap::new();
        let mut input_tokens = 0;
        for question in prepared {
            let (tokens, markers) = self.laya.tokenize(
                &self.engine.backing_cart().language_model,
                &request.state,
                &question,
            )?;
            input_tokens += tokens.len();
            let mut chunks = TokenizerChunks::new();
            chunks.append(TokenizerChunk::new_text(tokens));
            let column = question.column();
            let scores = self.engine.with_mut_return(move |engine| {
                engine.reset_context().map_err(ReadError::from)?;
                engine.read_chunks(chunks)?;
                markers
                    .iter()
                    .map(|&marker| {
                        let output = engine.ctx.embeddings_ith(marker as i32)?;
                        output.get(column).copied().ok_or_else(|| {
                            DecisionError::InvalidOutput("missing question-type score".into())
                        })
                    })
                    .collect::<Result<Vec<_>, DecisionError>>()
            })?;
            let probabilities = softmax(&scores, self.laya.temperature(&question))?;
            answers.insert(question.id.to_owned(), question.answer(probabilities));
        }
        Ok(DecisionResponse {
            answers,
            usage: DecisionUsage {
                input_tokens,
                output_tokens: 0,
            },
        })
    }
}

struct LayaState {
    template: String,
    marker: LlamaToken,
    sep: LlamaToken,
    marker_text: String,
    max_head_tokens: usize,
    temperatures: HashMap<String, f64>,
}

impl LayaState {
    fn new(model: &LlamaModel) -> Result<Self, DecisionError> {
        let architecture = model
            .meta_val_str("general.architecture")
            .unwrap_or_default();
        let prefix = format!("{architecture}.decision.");
        let decision_type = model
            .meta_val_str(&format!("{prefix}type"))
            .unwrap_or_else(|_| "none".into());
        if decision_type != "laya" {
            return Err(DecisionError::UnsupportedModel { decision_type });
        }
        let template = model
            .meta_val_str("tokenizer.chat_template.systemone")
            .map_err(|_| DecisionError::InvalidModel("missing systemone template".into()))?;
        LAYA_ENV.template_from_str(&template)?;
        let max_head_tokens = model
            .meta_val_str(&format!("{prefix}max_head_tokens"))
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|&n| n > 0 && n <= model.n_ctx_train() as usize)
            .ok_or_else(|| {
                DecisionError::InvalidModel("missing or invalid max_head_tokens".into())
            })?;
        let vocab = model.vocab();
        let marker = vocab.mask();
        let sep = vocab.sep();
        if marker.0 < 0 || sep.0 < 0 || model.n_embd_out() != 3 {
            return Err(DecisionError::InvalidModel(
                "expected mask/sep tokens and three decision output columns".into(),
            ));
        }
        let marker_text = String::from_utf8(vocab.token_to_piece(marker, true, None))
            .map_err(|_| DecisionError::InvalidModel("invalid option marker text".into()))?;
        if marker_text.is_empty() {
            return Err(DecisionError::InvalidModel(
                "empty option marker text".into(),
            ));
        }
        let mut temperatures = HashMap::new();
        let temperature_prefix = format!("{prefix}temperature.");
        for i in 0..model.meta_count() {
            let key = model
                .meta_key_by_index(i)
                .map_err(|error| DecisionError::InvalidModel(error.to_string()))?;
            if let Some(name) = key.strip_prefix(&temperature_prefix) {
                let temperature = model
                    .meta_val_str(&key)
                    .ok()
                    .and_then(|value| value.parse::<f64>().ok())
                    .filter(|value| value.is_finite() && *value > 0.0)
                    .ok_or_else(|| {
                        DecisionError::InvalidModel(format!("invalid temperature: {key}"))
                    })?;
                temperatures.insert(name.to_owned(), temperature);
            }
        }
        Ok(Self {
            template,
            marker,
            sep,
            marker_text,
            max_head_tokens,
            temperatures,
        })
    }

    fn temperature(&self, question: &PreparedQuestion<'_>) -> f64 {
        // GGUF temperatures calibrate probabilities by question type and option count.
        // Prefer a bucket-specific value, then a type-wide value, then 1.0.
        let bucket = match question.options.len() {
            0..=2 => "2",
            3..=5 => "3_5",
            6..=10 => "6_10",
            _ => "11",
        };
        let kind = question.kind_name();
        self.temperatures
            .get(&format!("{kind}.{bucket}"))
            .or_else(|| self.temperatures.get(kind))
            .copied()
            .unwrap_or(1.0)
    }

    fn tokenize(
        &self,
        model: &LlamaModel,
        state: &Value,
        question: &PreparedQuestion<'_>,
    ) -> Result<(Vec<LlamaToken>, Vec<usize>), DecisionError> {
        let options = question
            .options
            .iter()
            .map(|(key, description)| json!({"key": key, "description": description}))
            .collect::<Vec<_>>();
        let mut context = json!({
            "id": question.id, "type": question.kind_name(), "state": state,
            "instructions": question.instructions, "options": options, "images": [],
        });
        replace_marker(&mut context, &self.marker_text);
        let prompt = LAYA_ENV
            .template_from_str(&self.template)?
            .render(context)?;
        let tokens = model.vocab().tokenize(prompt.as_bytes(), false, true);
        self.truncate_head(tokens, question.options.len())
    }

    // Match llama.cpp's Laya head truncation; never truncate the state.
    fn truncate_head(
        &self,
        tokens: Vec<LlamaToken>,
        n_options: usize,
    ) -> Result<(Vec<LlamaToken>, Vec<usize>), DecisionError> {
        let markers = tokens
            .iter()
            .enumerate()
            .filter_map(|(i, &token)| (token == self.marker).then_some(i))
            .collect::<Vec<_>>();
        let invalid_layout = || {
            DecisionError::InvalidRequest(
                "unexpected Laya prompt layout; input may contain reserved special tokens".into(),
            )
        };
        if markers.len() != n_options
            || markers[0] < 2
            || tokens[markers[0] - 1] != self.sep
            || tokens.last() != Some(&self.sep)
        {
            return Err(invalid_layout());
        }
        let head_end = markers[0] - 1;
        let options_end = (markers[n_options - 1]..tokens.len())
            .find(|&i| tokens[i] == self.sep)
            .ok_or_else(invalid_layout)?;
        if options_end + 1 >= tokens.len() {
            return Err(invalid_layout());
        }
        let mut options = markers
            .iter()
            .enumerate()
            .map(|(i, &start)| {
                let end = markers.get(i + 1).copied().unwrap_or(options_end);
                tokens[start..end]
                    .iter()
                    .take(49)
                    .copied()
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let mut n_option_tokens: usize = options.iter().map(Vec::len).sum();
        if n_option_tokens + 16 > self.max_head_tokens {
            let limit = (self.max_head_tokens.saturating_sub(16) / n_options).max(4);
            for option in &mut options {
                option.truncate(limit);
            }
            n_option_tokens = options.iter().map(Vec::len).sum();
        }
        let max_question = self.max_head_tokens.saturating_sub(n_option_tokens).max(8);
        let mut output = vec![tokens[0]];
        output.extend_from_slice(&tokens[1..head_end.min(1 + max_question)]);
        output.push(self.sep);
        let mut output_markers = Vec::with_capacity(n_options);
        for option in options {
            output_markers.push(output.len());
            output.extend(option);
        }
        output.extend_from_slice(&tokens[options_end..]);
        Ok((output, output_markers))
    }
}

fn replace_marker(value: &mut Value, marker: &str) {
    match value {
        Value::String(text) => *text = text.replace(marker, " "),
        Value::Array(values) => values
            .iter_mut()
            .for_each(|value| replace_marker(value, marker)),
        Value::Object(values) => values
            .values_mut()
            .for_each(|value| replace_marker(value, marker)),
        _ => {}
    }
}

pub(super) fn softmax(scores: &[f32], temperature: f64) -> Result<Vec<f64>, DecisionError> {
    if scores.is_empty()
        || scores.iter().any(|score| !score.is_finite())
        || !temperature.is_finite()
        || temperature <= 0.0
    {
        return Err(DecisionError::InvalidOutput(
            "expected finite scores and a positive temperature".into(),
        ));
    }
    let maximum = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mut probabilities = scores
        .iter()
        .map(|score| f64::from(*score - maximum) / temperature)
        .map(f64::exp)
        .collect::<Vec<_>>();
    let total: f64 = probabilities.iter().sum();
    for probability in &mut probabilities {
        *probability /= total;
    }
    Ok(probabilities)
}
