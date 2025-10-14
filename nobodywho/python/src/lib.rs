use nobodywho::{
    chat::{self, ChatBuilder, TokenStream},
    llm,
    sampler_config::SamplerConfig,
};
use pyo3::prelude::*;
use pyo3_async_runtimes::tokio::{future_into_py, get_runtime};
use std::sync::Arc;

#[pyclass]
pub struct NobodyWhoModel {
    model: Option<llm::Model>,
}

#[pymethods]
impl NobodyWhoModel {
    #[new]
    pub fn new(model_path: &str) -> Self {
        Self {
            model: match llm::get_model(model_path, true) {
                Ok(model) => Some(model),
                _ => None,
            },
        }
    }
}

#[pyclass]
pub struct NobodyWhoTokenStream {
    tokens: TokenStream,
}

#[pymethods]
impl NobodyWhoTokenStream {
    pub fn next_token(&mut self) -> Option<String> {
        self.tokens.next_token_sync()
    }
}

#[pyclass]
pub struct NobodyWhoChatBuilder {
    model: Option<llm::Model>,
    n_ctx: u32,
    system_prompt: String,
}

#[pymethods]
impl NobodyWhoChatBuilder {
    #[new]
    pub fn new(model: &NobodyWhoModel) -> PyResult<Self> {
        if let Some(ref model) = model.model {
            Ok(Self {
                model: Some(Arc::clone(model)),
                n_ctx: 2048,
                system_prompt: String::new(),
            })
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Model is not initialized",
            ))
        }
    }

    pub fn with_context_size(&mut self, n_ctx: u32) -> PyResult<Self> {
        self.n_ctx = n_ctx;
        Ok(Self {
            model: self.model.clone(),
            n_ctx: self.n_ctx,
            system_prompt: self.system_prompt.clone(),
        })
    }

    pub fn with_system_prompt(&mut self, prompt: String) -> PyResult<Self> {
        self.system_prompt = prompt;
        Ok(Self {
            model: self.model.clone(),
            n_ctx: self.n_ctx,
            system_prompt: self.system_prompt.clone(),
        })
    }

    pub fn build(&self) -> PyResult<NobodyWhoChat> {
        if let Some(ref model) = self.model {
            let chat_builder = chat::ChatBuilder::new(Arc::clone(model))
                .with_context_size(self.n_ctx)
                .with_system_prompt(&self.system_prompt);

            Ok(NobodyWhoChat {
                chat_handle: Some(chat_builder.build()),
            })
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Model is not initialized",
            ))
        }
    }
}

#[pyclass]
pub struct NobodyWhoChat {
    chat_handle: Option<chat::ChatHandle>,
}

#[pymethods]
impl NobodyWhoChat {
    pub fn say_complete_blocking(&self, text: String) -> PyResult<String> {
        if let Some(ref handle) = self.chat_handle {
            // Use tokio runtime to block on the async operation
            let runtime = tokio::runtime::Runtime::new()
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))?;

            runtime.block_on(async {
                handle.say_complete(text).await.map_err(|e| {
                    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e))
                })
            })
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Chat handle not initialized",
            ))
        }
    }

    pub fn say_stream(&self, text: String) -> PyResult<NobodyWhoTokenStream> {
        if let Some(ref handle) = self.chat_handle {
            return Ok(NobodyWhoTokenStream {
                tokens: handle.say_stream(text),
            });
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Chat handle not initialized",
            ))
        }
    }
}

#[pymodule]
fn python(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<NobodyWhoModel>()?;
    m.add_class::<NobodyWhoChatBuilder>()?;
    m.add_class::<NobodyWhoChat>()?;
    m.add_class::<NobodyWhoTokenStream>()?;
    Ok(())
}
