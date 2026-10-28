//! Typed, non-generative decisions over text or JSON. Currently supports Laya GGUFs.
//!
//! Questions run independently. Laya's token budgets apply to questions and options;
//! oversized states are rejected. Probabilities use the model's fitted temperatures,
//! which may need recalibration for your data.
//!
//! ```no_run
//! use nobodywho::{decision::{Decision, DecisionRequest}, llm::get_model};
//! use std::sync::Arc;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let model = Arc::new(get_model(
//!     "hf://ggml-org/Laya-GGUF/Laya-Q8_0.gguf", true, None, None, None,
//! )?);
//! let decisions = Decision::new(model, 1024)?;
//! let request: DecisionRequest = serde_json::from_str(r#"{
//!     "state": "I was charged twice. Please refund the duplicate payment.",
//!     "questions": {
//!         "team": {"type": "choice", "instructions": "Which team should handle this?",
//!                  "criteria": {"billing": "payments and refunds", "support": "technical issues"}},
//!         "refund": {"type": "noul", "instructions": "Is a refund requested?"}
//!     }
//! }"#)?;
//! let response = decisions.decide(request)?;
//! println!("{:?}", response.answers);
//! # Ok(())
//! # }
//! ```

use crate::errors::DecisionError;
use crate::llm::Model;
use crate::worker::AsyncWorker;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
mod laya;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionRequest {
    pub state: Value,
    pub questions: IndexMap<String, DecisionQuestion>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum DecisionQuestion {
    Choice {
        instructions: Value,
        /// Option IDs mapped to descriptions, or null for bare labels.
        criteria: IndexMap<String, Value>,
    },
    Score {
        instructions: Value,
        /// Ordered descriptions, from lowest to highest, with 2 to 10 levels.
        criteria: Vec<Value>,
    },
    #[serde(rename = "noul")]
    Boolean {
        instructions: Value,
        /// Optional descriptions keyed by "false" and "true".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        criteria: Option<IndexMap<String, Value>>,
    },
}

struct PreparedQuestion<'a> {
    id: &'a str,
    question: &'a DecisionQuestion,
    instructions: &'a Value,
    options: Vec<(String, &'a Value)>,
}

impl<'a> PreparedQuestion<'a> {
    fn new(id: &'a str, question: &'a DecisionQuestion) -> Result<Self, DecisionError> {
        let (instructions, options) = match question {
            DecisionQuestion::Choice {
                instructions,
                criteria,
            } => (
                instructions,
                criteria
                    .iter()
                    .map(|(key, value)| (key.clone(), value))
                    .collect(),
            ),
            DecisionQuestion::Score {
                instructions,
                criteria,
            } => {
                if !(2..=10).contains(&criteria.len()) {
                    return Err(DecisionError::InvalidRequest(format!(
                        "question {id:?}: score requires 2 to 10 levels"
                    )));
                }
                (
                    instructions,
                    criteria
                        .iter()
                        .enumerate()
                        .map(|(i, value)| (i.to_string(), value))
                        .collect(),
                )
            }
            DecisionQuestion::Boolean {
                instructions,
                criteria,
            } => {
                if criteria.as_ref().is_some_and(|criteria| {
                    criteria.keys().any(|key| key != "false" && key != "true")
                }) {
                    return Err(DecisionError::InvalidRequest(format!(
                        "question {id:?}: Boolean criteria must use 'false' and 'true' keys"
                    )));
                }
                (
                    instructions,
                    ["false", "true"]
                        .into_iter()
                        .map(|key| {
                            (
                                key.to_owned(),
                                criteria
                                    .as_ref()
                                    .and_then(|criteria| criteria.get(key))
                                    .unwrap_or(&Value::Null),
                            )
                        })
                        .collect(),
                )
            }
        };
        let options: Vec<_> = options;
        if id.is_empty() || instructions.is_null() || options.is_empty() || options.len() > 255 {
            return Err(DecisionError::InvalidRequest(format!(
                "question {id:?}: a nonempty ID, instructions, and 1 to 255 options are required"
            )));
        }
        Ok(Self {
            id,
            question,
            instructions,
            options,
        })
    }

    fn kind_name(&self) -> &'static str {
        match self.question {
            DecisionQuestion::Choice { .. } => "choice",
            DecisionQuestion::Score { .. } => "score",
            DecisionQuestion::Boolean { .. } => "noul",
        }
    }

    fn answer(&self, probabilities: Vec<f64>) -> DecisionAnswer {
        let is_choice = match self.question {
            DecisionQuestion::Boolean { .. } => {
                return DecisionAnswer::Boolean {
                    noul: probabilities[1],
                };
            }
            DecisionQuestion::Choice { .. } => true,
            DecisionQuestion::Score { .. } => false,
        };
        let best = probabilities
            .iter()
            .enumerate()
            .rev()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .map_or(0, |(i, _)| i);
        let distribution = self
            .options
            .iter()
            .zip(&probabilities)
            .map(|((key, _), &probability)| (key.clone(), probability))
            .collect();
        let uniform = 1.0 / probabilities.len() as f64;
        if is_choice {
            let confidence = if probabilities.len() == 1 {
                1.0
            } else {
                ((probabilities[best] - uniform) / (1.0 - uniform)).max(0.0)
            };
            return DecisionAnswer::Choice {
                choice: self.options[best].0.clone(),
                probabilities: distribution,
                confidence,
            };
        }
        let score = probabilities
            .iter()
            .enumerate()
            .map(|(i, p)| i as f64 * p)
            .sum();
        let distance: f64 = probabilities
            .iter()
            .enumerate()
            .map(|(i, p)| p * (i as f64 - best as f64).abs())
            .sum();
        let uniform_distance: f64 = (0..probabilities.len())
            .map(|i| (i as f64 - (probabilities.len() - 1) as f64 / 2.0).abs() * uniform)
            .sum();
        DecisionAnswer::Score {
            score,
            legend: self
                .options
                .iter()
                .map(|(key, value)| (key.clone(), (*value).clone()))
                .collect(),
            probabilities: distribution,
            confidence: (1.0 - distance / uniform_distance).max(0.0),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DecisionResponse {
    pub answers: IndexMap<String, DecisionAnswer>,
    pub usage: DecisionUsage,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum DecisionAnswer {
    Choice {
        choice: String,
        probabilities: IndexMap<String, f64>,
        /// Separation from a uniform distribution, not a probability of correctness.
        confidence: f64,
    },
    Score {
        /// Expected zero-based level index, which can fall between levels.
        score: f64,
        legend: IndexMap<String, Value>,
        probabilities: IndexMap<String, f64>,
        confidence: f64,
    },
    #[serde(rename = "noul")]
    Boolean { noul: f64 },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DecisionUsage {
    /// Sum of evaluated prompt lengths across questions, after head truncation.
    pub input_tokens: usize,
    pub output_tokens: usize,
}

pub struct Decision {
    laya: tokio::sync::Mutex<laya::Laya>,
}

impl Decision {
    pub fn new(model: Arc<Model>, n_ctx: u32) -> Result<Self, DecisionError> {
        let laya = tokio::sync::Mutex::new(laya::Laya::new(model, n_ctx)?);
        Ok(Self { laya })
    }

    pub fn decide(&self, request: DecisionRequest) -> Result<DecisionResponse, DecisionError> {
        self.laya.blocking_lock().decide(request)
    }
}

#[derive(Clone)]
pub struct DecisionAsync {
    laya: Arc<tokio::sync::Mutex<laya::Laya>>,
    worker: Arc<AsyncWorker>,
}

impl DecisionAsync {
    pub fn new(model: Arc<Model>, n_ctx: u32) -> Result<Self, DecisionError> {
        let laya = laya::Laya::new(model, n_ctx)?;
        Ok(Self {
            laya: Arc::new(tokio::sync::Mutex::new(laya)),
            worker: Arc::new(AsyncWorker::new()),
        })
    }

    pub async fn decide(
        &self,
        request: DecisionRequest,
    ) -> Result<DecisionResponse, DecisionError> {
        let mut laya = Arc::clone(&self.laya).lock_owned().await;
        self.worker.run(move || laya.decide(request)).await
    }
}
