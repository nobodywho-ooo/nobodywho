use pyo3::prelude::*;

#[pyclass]
pub struct NobodyWhoModel {
    model: nobodywho::llm::Model,
}

#[pymethods]
impl NobodyWhoModel {
    #[new]
    #[pyo3(signature = (model_path, use_gpu_if_available = true))]
    pub fn new(model_path: &str, use_gpu_if_available: bool) -> PyResult<Self> {
        let model_result = nobodywho::llm::get_model(model_path, use_gpu_if_available);
        match model_result {
            Ok(model) => Ok(Self { model }),
            Err(err) => Err(pyo3::exceptions::PyRuntimeError::new_err(err.to_string())),
        }
    }
}

#[pyclass]
pub struct NobodyWhoTokenStream {
    tokens: nobodywho::chat::TokenStream,
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
    chat_handle: nobodywho::chat::ChatHandle,
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
    ) -> Self {
        let chat_handle = nobodywho::chat::ChatBuilder::new(model.model.clone())
            .with_context_size(n_ctx)
            .with_tools(vec![])
            .with_allow_thinking(allow_thinking)
            .with_system_prompt(system_prompt)
            .build();
        Self { chat_handle }
    }

    pub fn say_complete(&self, text: String, py: Python) -> PyResult<String> {
        let handle = &self.chat_handle;
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
    }

    async fn say_complete_async(&self, text: String) -> PyResult<String> {
        let handle = &self.chat_handle;
        handle
            .say_complete(text)
            .await
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))
    }

    pub fn say_stream(&self, text: String) -> NobodyWhoTokenStream {
        let handle = &self.chat_handle;
        NobodyWhoTokenStream {
            tokens: handle.say_stream(text),
        }
    }
}

#[pymodule]
#[pyo3(name = "nobodywho")]
fn nobodywhopython(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<NobodyWhoModel>()?;
    m.add_class::<NobodyWhoChat>()?;
    m.add_class::<NobodyWhoTokenStream>()?;
    Ok(())
}
