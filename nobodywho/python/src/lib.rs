use pyo3::prelude::*;

#[pyclass]
pub struct Model {
    model: nobodywho::llm::Model,
}

#[pymethods]
impl Model {
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
pub struct TokenStream {
    stream: nobodywho::chat::TokenStream,
}

#[pymethods]
impl TokenStream {
    pub fn next_token_blocking(&mut self, py: Python) -> Option<String> {
        // Release the GIL while waiting for the next token
        // This allows the background thread to acquire the GIL if needed for tool calls
        py.detach(|| self.stream.next_token_sync())
    }

    async fn next_token(&mut self) -> Option<String> {
        // Currently deattaching is not needed here. Noting that this might change later
        self.stream.next_token().await
    }

    fn collect_blocking(&mut self, py: Python) -> String {
        py.detach(|| futures::executor::block_on(self.collect()))
    }

    async fn collect(&mut self) -> String {
        self.stream.collect().await
    }

    // sync iterator stuff
    pub fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    pub fn __next__(mut slf: PyRefMut<'_, Self>, py: Python) -> Option<String> {
        slf.next_token_blocking(py)
    }

    // TODO: async iterator (turns out to be trickier than expected)
}

#[pyclass]
pub struct Embeddings {
    embeddings_handle: nobodywho::embed::EmbeddingsHandle,
}

#[pymethods]
impl Embeddings {
    #[new]
    #[pyo3(signature = (model, n_ctx = 2048))]
    pub fn new(model: &Model, n_ctx: u32) -> Self {
        let embeddings_handle = nobodywho::embed::EmbeddingsHandle::new(model.model.clone(), n_ctx);
        Self { embeddings_handle }
    }

    pub fn embed_text_blocking(&self, text: String, py: Python) -> PyResult<Vec<f32>> {
        py.detach(|| futures::executor::block_on(self.embed_text(text)))
    }

    async fn embed_text(&self, text: String) -> PyResult<Vec<f32>> {
        let mut rx = self.embeddings_handle.embed_text(text);
        rx.recv().await.ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Failed to receive embedding")
        })
    }
}

#[pyclass]
pub struct CrossEncoder {
    crossencoder_handle: nobodywho::crossencoder::CrossEncoderHandle,
}

#[pymethods]
impl CrossEncoder {
    #[new]
    #[pyo3(signature = (model, n_ctx = 2048))]
    pub fn new(model: &Model, n_ctx: u32) -> Self {
        let crossencoder_handle =
            nobodywho::crossencoder::CrossEncoderHandle::new(model.model.clone(), n_ctx);
        Self {
            crossencoder_handle,
        }
    }

    pub fn rank_blocking(
        &self,
        query: String,
        documents: Vec<String>,
        py: Python,
    ) -> PyResult<Vec<f32>> {
        py.detach(|| futures::executor::block_on(self.rank(query, documents)))
    }

    async fn rank(&self, query: String, documents: Vec<String>) -> PyResult<Vec<f32>> {
        let mut rx = self.crossencoder_handle.rank(query, documents);
        rx.recv().await.ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Failed to receive ranking scores")
        })
    }

    pub fn rank_and_sort_blocking(
        &self,
        query: String,
        documents: Vec<String>,
        py: Python,
    ) -> PyResult<Vec<(String, f32)>> {
        py.detach(|| futures::executor::block_on(self.rank_and_sort(query, documents)))
    }

    async fn rank_and_sort(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> PyResult<Vec<(String, f32)>> {
        self.crossencoder_handle
            .rank_and_sort(query, documents)
            .await
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))
    }
}

#[pyclass]
pub struct Chat {
    chat_handle: nobodywho::chat::ChatHandle,
}

#[pymethods]
impl Chat {
    #[new]
    #[pyo3(signature = (model, n_ctx = 2048, system_prompt = "", allow_thinking = true))]
    pub fn new(model: &Model, n_ctx: u32, system_prompt: &str, allow_thinking: bool) -> Self {
        let chat_handle = nobodywho::chat::ChatBuilder::new(model.model.clone())
            .with_context_size(n_ctx)
            .with_tools(vec![])
            .with_allow_thinking(allow_thinking)
            .with_system_prompt(system_prompt)
            .build();
        Self { chat_handle }
    }

    pub fn send_message(&self, text: String) -> TokenStream {
        TokenStream {
            stream: self.chat_handle.say_stream(text),
        }
    }
}

#[pyfunction]
fn cosine_similarity(a: Vec<f32>, b: Vec<f32>) -> PyResult<f32> {
    if a.len() != b.len() {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "Vectors must have the same length",
        ));
    }
    Ok(nobodywho::embed::cosine_similarity(&a, &b))
}

#[pyclass]
pub struct Tool {
    tool: nobodywho::chat::Tool,
}

#[pymethods]
impl Tool {
    #[new]
    #[pyo3(signature = (fun))]
    pub fn new(fun: Py<PyAny>) -> Self {
        let name = "foobar";
        let description = "foobar";
        let json_schema = serde_json::json!( { "foo": "bar" });

        // wrap the passed function in a json -> String function
        let function = move |json: serde_json::Value| {
            Python::attach(|py| {
                // TODO: construct proper kwargs
                let kwargs = pyo3::types::PyDict::new(py);

                // call the python function
                let py_result = fun.call(py, (), Some(&kwargs));

                // extract a string from the result
                // return an error string to the LLM if anything fails
                match py_result.map(|r| r.extract::<String>(py)) {
                    Ok(Ok(str)) => str,
                    Err(pyerr) | Ok(Err(pyerr)) => format!("ERROR: {pyerr}"),
                }
                .to_string()
            })
        };

        let tool = nobodywho::chat::Tool::new(
            name,
            description,
            json_schema,
            std::sync::Arc::new(function),
        );
        Self { tool }
    }
}

#[pymodule]
#[pyo3(name = "nobodywho")]
fn nobodywhopython(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Model>()?;
    m.add_class::<Chat>()?;
    m.add_class::<TokenStream>()?;
    m.add_class::<Embeddings>()?;
    m.add_class::<CrossEncoder>()?;
    m.add_function(wrap_pyfunction!(cosine_similarity, m)?)?;
    Ok(())
}
