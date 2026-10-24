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
use crate::llm::{Model, WorkerGuard};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::oneshot;
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

#[derive(Clone)]
pub struct Decision {
    async_handle: DecisionAsync,
}

#[derive(Clone)]
pub struct DecisionAsync {
    guard: Arc<WorkerGuard<DecisionMsg>>,
}

impl Decision {
    pub fn new(model: Arc<Model>, n_ctx: u32) -> Result<Self, DecisionError> {
        Ok(Self {
            async_handle: DecisionAsync::new(model, n_ctx)?,
        })
    }

    pub fn decide(&self, request: DecisionRequest) -> Result<DecisionResponse, DecisionError> {
        futures::executor::block_on(self.async_handle.decide(request))
    }
}

impl DecisionAsync {
    pub fn new(model: Arc<Model>, n_ctx: u32) -> Result<Self, DecisionError> {
        let mut worker = laya::LayaWorker::new(model, n_ctx)?;
        let (msg_tx, msg_rx) = std::sync::mpsc::channel::<DecisionMsg>();
        let join_handle = std::thread::Builder::new()
            .name("nobodywho-decisions".into())
            .spawn(move || {
                while let Ok(msg) = msg_rx.recv() {
                    let result = worker.decide(msg.request);
                    let _ = msg.output_tx.send(result);
                }
            })?;
        Ok(Self {
            guard: Arc::new(WorkerGuard::new(msg_tx, join_handle, None)),
        })
    }

    pub async fn decide(
        &self,
        request: DecisionRequest,
    ) -> Result<DecisionResponse, DecisionError> {
        let (output_tx, output_rx) = oneshot::channel();
        if !self.guard.send(DecisionMsg { request, output_tx }) {
            return Err(DecisionError::WorkerTerminated);
        }
        output_rx
            .await
            .map_err(|_| DecisionError::WorkerTerminated)?
    }
}

struct DecisionMsg {
    request: DecisionRequest,
    output_tx: oneshot::Sender<Result<DecisionResponse, DecisionError>>,
}
