use nobodywho::{
    chat::{self, ChatBuilder, TokenStream},
    llm,
};

use pyo3::prelude::*;

use std::sync::Arc;

#[pyclass]
pub struct NobodyWhoModel {
    model: Option<llm::Model>,
}

#[pymethods]
impl NobodyWhoModel {
    #[new]
    #[pyo3(signature = (model_path, use_gpu_if_available = true))]
    pub fn new(model_path: &str, use_gpu_if_available: bool) -> Self {
        Self {
            model: match llm::get_model(model_path, use_gpu_if_available) {
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
    pub fn next_token(&mut self, py: Python) -> Option<String> {
        // Release the GIL while waiting for the next token
        // This allows the background thread to acquire the GIL if needed for tool calls
        py.detach(|| self.tokens.next_token_sync())
    }

    async fn next_token_async(&mut self) -> Option<String> {
        // Currently deattaching is not needed here. Noting that this might change later
        self.tokens.next_token().await
    }
}

#[pyclass]
pub struct NobodyWhoChat {
    chat_handle: Option<chat::ChatHandle>,
}

#[pymethods]
impl NobodyWhoChat {
    #[new]
    #[pyo3(signature = (model, n_ctx = 2048, system_prompt = "", allow_thinking = true))]
    pub fn new(
        model: &NobodyWhoModel,
        n_ctx: u32,
        system_prompt: &str,
        allow_thinking: bool,
    ) -> PyResult<Self> {
        if let Some(ref model) = model.model {
            let cb = ChatBuilder::new(Arc::clone(model));
            Ok(Self {
                chat_handle: Some(
                    cb.with_context_size(n_ctx)
                        .with_system_prompt(system_prompt)
                        .with_tools(vec![])
                        .with_allow_thinking(allow_thinking)
                        .build(),
                ),
            })
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Model is not initialized",
            ))
        }
    }

    pub fn say_complete(&self, text: String, py: Python) -> PyResult<String> {
        if let Some(ref handle) = self.chat_handle {
            // Use tokio runtime to block on the async operation
            let runtime = tokio::runtime::Runtime::new()
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))?;
            // Release the GIL while generating response.
            // This allows the background thread to aquire it for potential tool calls
            py.detach(|| {
                runtime.block_on(async {
                    handle.say_complete(text).await.map_err(|e| {
                        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e))
                    })
                })
            })
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Chat handle not initialized",
            ))
        }
    }

    async fn say_complete_async(&self, text: String) -> PyResult<String> {
        if let Some(ref handle) = self.chat_handle {
            handle
                .say_complete(text)
                .await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Chat handle not initialized",
            ))
        }
    }

    pub fn say_stream(&self, text: String) -> PyResult<NobodyWhoTokenStream> {
        if let Some(ref handle) = self.chat_handle {
            Ok(NobodyWhoTokenStream {
                tokens: handle.say_stream(text),
            })
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Chat handle not initialized",
            ))
        }
    }
}

#[pymodule]
fn nobodywhopython(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<NobodyWhoModel>()?;
    m.add_class::<NobodyWhoChat>()?;
    m.add_class::<NobodyWhoTokenStream>()?;
    Ok(())
}
