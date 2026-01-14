use pyo3::prelude::*;

/// `Model` objects contain a GGUF model. It is primarily useful for sharing a single model instance
/// between multiple `Chat`, `Encoder`, or `CrossEncoder` instances.
/// Sharing is efficient because the underlying model data is reference-counted.
/// There is no `ModelAsync` variant. A regular `Model` can be used with both `Chat` and `ChatAsync`.
#[pyclass]
pub struct Model {
    model: nobodywho::llm::Model,
}

#[pymethods]
impl Model {
    /// Create a new Model from a GGUF file.
    ///
    /// Args:
    ///     model_path: Path to the GGUF model file
    ///     use_gpu_if_available: If True, attempts to use GPU acceleration. Defaults to True.
    ///
    /// Returns:
    ///     A Model instance
    ///
    /// Raises:
    ///     RuntimeError: If the model file cannot be loaded
    #[new]
    #[pyo3(signature = (model_path: "os.PathLike | str", use_gpu_if_available = true) -> "Model")]
    pub fn new(model_path: std::path::PathBuf, use_gpu_if_available: bool) -> PyResult<Self> {
        let path_str = model_path.to_str().ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "Path contains invalid UTF-8: {}",
                model_path.display()
            ))
        })?;
        let model_result = nobodywho::llm::get_model(path_str, use_gpu_if_available);
        match model_result {
            Ok(model) => Ok(Self { model }),
            Err(err) => Err(pyo3::exceptions::PyRuntimeError::new_err(err.to_string())),
        }
    }
}

/// This type represents a `Model | str` from python
/// The intent is to allow passing a string path directly to constructors like Chat
/// to make the simplest possible usage of nobodywho even simpler
/// i.e. `Chat("./model.gguf")` instead of `Chat(Model("./model.gguf"))`
#[derive(FromPyObject)]
pub enum ModelOrPath<'py> {
    ModelObj(Bound<'py, Model>),
    Path(std::path::PathBuf),
}

impl<'py> ModelOrPath<'py> {
    /// returns nobodywho core's internal model struct from a python `str | Model`
    fn get_inner_model(&self) -> PyResult<nobodywho::llm::Model> {
        match self {
            // the inner model is Arc<...>, so clone is cheap.
            ModelOrPath::ModelObj(model_obj) => Ok(model_obj.borrow().model.clone()),
            // default to (trying to) use GPU if a string is passed
            ModelOrPath::Path(path) => {
                let path_str = path.to_str().ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err(format!(
                        "Path contains invalid UTF-8: {}",
                        path.display()
                    ))
                })?;
                nobodywho::llm::get_model(path_str, true)
                    .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
            }
        }
    }
}

/// `TokenStream` represents an in-progress text completion. It is the return value of `Chat.ask`.
/// You can iterate over the tokens in a `TokenStream` using the normal python iterator protocol,
/// or by explicitly calling the `.next_token()` method.
/// If you want to wait for the entire response to be generated, you can call `.completed()`.
/// Also see `TokenStreamAsync`, for an async version of this class.
#[pyclass]
pub struct TokenStream {
    stream: nobodywho::chat::TokenStream,
}

#[pymethods]
impl TokenStream {
    /// Get the next token from the stream. Blocks until a token is available.
    ///
    /// Returns:
    ///     The next token as a string, or None if the stream has ended.
    #[pyo3(signature = () -> "str | None")]
    pub fn next_token(&mut self, py: Python) -> Option<String> {
        // Release the GIL while waiting for the next token
        // This allows the background thread to acquire the GIL if needed for tool calls
        py.detach(|| self.stream.next_token())
    }

    /// Wait for the entire response to be generated and return it as a single string.
    /// This blocks until generation is complete.
    ///
    /// Returns:
    ///     The complete generated text.
    ///
    /// Raises:
    ///     RuntimeError: If generation fails.
    pub fn completed(&mut self, py: Python) -> PyResult<String> {
        py.detach(|| self.stream.completed())
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    // sync iterator stuff
    pub fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    pub fn __next__(&mut self, py: Python) -> Option<String> {
        py.detach(|| self.stream.next_token())
    }
}

/// `TokenStreamAsync` is the async variant of the `TokenStream` class.
/// It has the same methods as `TokenStream`, but all methods must be awaited.
/// This class also supports async iteration using `async for token in stream:` syntax.
#[pyclass]
pub struct TokenStreamAsync {
    // this needs to be behind a mutex for async iterators to work
    // because __anext__ needs to return a python awaitable for *one* element
    // and our single-consumer channels can't be cloned
    stream: std::sync::Arc<tokio::sync::Mutex<nobodywho::chat::TokenStreamAsync>>,
    // we can probably get rid of this Arc<Mutex<...>>, if we switch to mpmc channels
    // (e.g. via the async_channel crate)
}

#[pymethods]
impl TokenStreamAsync {
    /// Get the next token from the stream asynchronously.
    ///
    /// Returns:
    ///     The next token as a string, or None if the stream has ended.
    #[pyo3(signature = () -> "typing.Awaitable[str | None]")]
    pub async fn next_token(&mut self) -> Option<String> {
        // no need to release GIL in async functions
        self.stream.lock().await.next_token().await
    }

    /// Wait for the entire response to be generated and return it as a single string.
    ///
    /// Returns:
    ///     The complete generated text.
    ///
    /// Raises:
    ///     RuntimeError: If generation fails.
    #[pyo3(signature = () -> "typing.Awaitable[str]")]
    pub async fn completed(&mut self) -> PyResult<String> {
        self.stream
            .lock()
            .await
            .completed()
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    pub fn __aiter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    pub fn __anext__<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyAny>> {
        let locals = pyo3_async_runtimes::TaskLocals::with_running_loop(py)?.copy_context(py)?;
        let stream_clone = self.stream.clone();
        pyo3_async_runtimes::tokio::future_into_py_with_locals(py, locals, async move {
            let token = stream_clone.lock().await.next_token().await;
            match token {
                Some(t) => Ok(t),
                None => Err(pyo3::exceptions::PyStopAsyncIteration::new_err(())),
            }
        })
    }
}

/// `Encoder` will let you generate vector representations of text.
/// It must be initialized with a model that specifically supports generating embeddings.
/// A regular chat/text-generation model will not just work.
/// Once initialized, you can call `.encode()` on a string, which returns a list of 32-bit floats.
/// See `EncoderAsync` for the async version of this class.
#[pyclass]
pub struct Encoder {
    encoder: nobodywho::encoder::Encoder,
}

#[pymethods]
impl Encoder {
    /// Create a new Encoder for generating text embeddings.
    ///
    /// Args:
    ///     model: An embedding model (Model instance or path to GGUF file)
    ///     n_ctx: Context size (maximum sequence length). Defaults to 4096.
    ///
    /// Returns:
    ///     An Encoder instance
    ///
    /// Raises:
    ///     RuntimeError: If the model cannot be loaded
    ///     ValueError: If the path contains invalid UTF-8
    #[new]
    #[pyo3(signature = (model: "Model | os.PathLike | str", n_ctx = 4096) -> "Encoder")]
    pub fn new(model: ModelOrPath, n_ctx: u32) -> PyResult<Self> {
        let nw_model = model.get_inner_model()?;
        let encoder = nobodywho::encoder::Encoder::new(nw_model, n_ctx);
        Ok(Self { encoder })
    }

    /// Generate an embedding vector for the given text. This method blocks until complete.
    ///
    /// Args:
    ///     text: The text to encode
    ///
    /// Returns:
    ///     A list of floats representing the embedding vector
    ///
    /// Raises:
    ///     RuntimeError: If encoding fails
    #[pyo3(signature = (text: "str") -> "list[float]")]
    pub fn encode(&self, text: String, py: Python) -> PyResult<Vec<f32>> {
        py.detach(|| {
            self.encoder
                .encode(text)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))
        })
    }
}

/// This is the async version of the `Encoder` class. See the docs on `Encoder` for more detail.
#[pyclass]
pub struct EncoderAsync {
    encoder_handle: nobodywho::encoder::EncoderAsync,
}

#[pymethods]
impl EncoderAsync {
    /// Create a new async Encoder for generating text embeddings.
    ///
    /// Args:
    ///     model: An embedding model (Model instance or path to GGUF file)
    ///     n_ctx: Context size (maximum sequence length). Defaults to 4096.
    ///
    /// Returns:
    ///     An EncoderAsync instance
    ///
    /// Raises:
    ///     RuntimeError: If the model cannot be loaded
    ///     ValueError: If the path contains invalid UTF-8
    #[new]
    #[pyo3(signature = (model: "Model | os.PathLike | str", n_ctx = 4096) -> "EncoderAsync")]
    pub fn new(model: ModelOrPath, n_ctx: u32) -> PyResult<Self> {
        let nw_model = model.get_inner_model()?;
        let encoder_handle = nobodywho::encoder::EncoderAsync::new(nw_model, n_ctx);
        Ok(Self { encoder_handle })
    }

    /// Generate an embedding vector for the given text asynchronously.
    ///
    /// Args:
    ///     text: The text to encode
    ///
    /// Returns:
    ///     A list of floats representing the embedding vector
    ///
    /// Raises:
    ///     RuntimeError: If encoding fails
    #[pyo3(signature = (text: "str") -> "typing.Awaitable[list[float]]")]
    async fn encode(&self, text: String) -> PyResult<Vec<f32>> {
        self.encoder_handle.encode(text).await.map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Failed to receive embedding: {e}"
            ))
        })
    }
}

/// A `CrossEncoder` is a kind of encoder that is trained to compare similarity between two texts.
/// It is particularly useful for searching a list of texts with a query, to find the closest one.
/// `CrossEncoder` requires a model made specifically for cross-encoding.
/// See `CrossEncoderAsync` for the async version of this class.
#[pyclass]
pub struct CrossEncoder {
    crossencoder: nobodywho::crossencoder::CrossEncoder,
}

#[pymethods]
impl CrossEncoder {
    /// Create a new CrossEncoder for comparing text similarity.
    ///
    /// Args:
    ///     model: A cross-encoder model (Model instance or path to GGUF file)
    ///     n_ctx: Context size (maximum sequence length). Defaults to 4096.
    ///
    /// Returns:
    ///     A CrossEncoder instance
    ///
    /// Raises:
    ///     RuntimeError: If the model cannot be loaded
    ///     ValueError: If the path contains invalid UTF-8
    #[new]
    #[pyo3(signature = (model: "Model | os.PathLike | str", n_ctx = 4096) -> "CrossEncoder")]
    pub fn new(model: ModelOrPath, n_ctx: u32) -> PyResult<Self> {
        let nw_model = model.get_inner_model()?;
        let crossencoder = nobodywho::crossencoder::CrossEncoder::new(nw_model, n_ctx);
        Ok(Self { crossencoder })
    }

    /// Compute similarity scores between a query and multiple documents. This method blocks.
    ///
    /// Args:
    ///     query: The query text
    ///     documents: List of documents to compare against the query
    ///
    /// Returns:
    ///     List of similarity scores (higher = more similar). Scores are in the same order as documents.
    ///
    /// Raises:
    ///     RuntimeError: If ranking fails
    #[pyo3(signature = (query: "str", documents: "list[str]") -> "list[float]")]
    pub fn rank(&self, query: String, documents: Vec<String>, py: Python) -> PyResult<Vec<f32>> {
        py.detach(|| {
            self.crossencoder
                .rank(query, documents)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))
        })
    }

    /// Rank documents by similarity to query and return them sorted. This method blocks.
    ///
    /// Args:
    ///     query: The query text
    ///     documents: List of documents to compare against the query
    ///
    /// Returns:
    ///     List of (document, score) tuples sorted by descending similarity (most similar first).
    ///
    /// Raises:
    ///     RuntimeError: If ranking fails
    #[pyo3(signature = (query: "str", documents: "list[str]") -> "list[tuple[str, float]]")]
    pub fn rank_and_sort(
        &self,
        query: String,
        documents: Vec<String>,
        py: Python,
    ) -> PyResult<Vec<(String, f32)>> {
        py.detach(|| {
            self.crossencoder
                .rank_and_sort(query, documents)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))
        })
    }
}

/// This is the async version of `CrossEncoder`.
/// See the docs for `CrossEncoder` for more details.
#[pyclass]
pub struct CrossEncoderAsync {
    crossencoder_handle: nobodywho::crossencoder::CrossEncoderAsync,
}

#[pymethods]
impl CrossEncoderAsync {
    /// Create a new async CrossEncoder for comparing text similarity.
    ///
    /// Args:
    ///     model: A cross-encoder model (Model instance or path to GGUF file)
    ///     n_ctx: Context size (maximum sequence length). Defaults to 4096.
    ///
    /// Returns:
    ///     A CrossEncoderAsync instance
    ///
    /// Raises:
    ///     RuntimeError: If the model cannot be loaded
    ///     ValueError: If the path contains invalid UTF-8
    #[new]
    #[pyo3(signature = (model: "Model | os.PathLike | str", n_ctx = 4096) -> "CrossEncoderAsync")]
    pub fn new(model: ModelOrPath, n_ctx: u32) -> PyResult<Self> {
        let nw_model = model.get_inner_model()?;
        let crossencoder_handle = nobodywho::crossencoder::CrossEncoderAsync::new(nw_model, n_ctx);
        Ok(Self {
            crossencoder_handle,
        })
    }

    /// Compute similarity scores between a query and multiple documents asynchronously.
    ///
    /// Args:
    ///     query: The query text
    ///     documents: List of documents to compare against the query
    ///
    /// Returns:
    ///     List of similarity scores (higher = more similar). Scores are in the same order as documents.
    ///
    /// Raises:
    ///     RuntimeError: If ranking fails
    #[pyo3(signature = (query: "str", documents: "list[str]") -> "typing.Awaitable[list[float]]")]
    async fn rank(&self, query: String, documents: Vec<String>) -> PyResult<Vec<f32>> {
        self.crossencoder_handle
            .rank(query, documents)
            .await
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Failed to receive ranking scores: {e}"
                ))
            })
    }

    /// Rank documents by similarity to query and return them sorted asynchronously.
    ///
    /// Args:
    ///     query: The query text
    ///     documents: List of documents to compare against the query
    ///
    /// Returns:
    ///     List of (document, score) tuples sorted by descending similarity (most similar first).
    ///
    /// Raises:
    ///     RuntimeError: If ranking fails
    #[pyo3(signature = (query: "str", documents: "list[str]") -> "typing.Awaitable[list[tuple[str, float]]]")]
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

/// `Chat` is a general-purpose class for interacting with instruction-tuned conversational LLMs.
/// It should be initialized with a turn-taking LLM, which includes a chat template.
/// On a `Chat` instance, you can call `.ask()` with the prompt you intend to pass to the model,
/// which returns a `TokenStream`, representing the generated response.
/// `Chat` also supports calling tools.
/// When initializing a `Chat`, you can also specify additional generation configuration, like
/// what tools to provide, what sampling strategy to use for choosing tokens, what system prompt
/// to use, whether to allow extended thinking, etc.
/// See `ChatAsync` for the async version of this class.
#[pyclass]
pub struct Chat {
    chat_handle: nobodywho::chat::ChatHandle,
}

#[pymethods]
impl Chat {
    /// Create a new Chat instance for conversational text generation.
    ///
    /// Args:
    ///     model: A chat model (Model instance or path to GGUF file)
    ///     n_ctx: Context size (maximum conversation length in tokens). Defaults to 4096.
    ///     system_prompt: System message to guide the model's behavior. Defaults to empty string.
    ///     allow_thinking: If True, allows extended reasoning tokens for supported models. Defaults to True.
    ///     tools: List of Tool instances the model can call. Defaults to empty list.
    ///     sampler: SamplerConfig for token selection. Defaults to SamplerConfig.default().
    ///
    /// Returns:
    ///     A Chat instance
    ///
    /// Raises:
    ///     RuntimeError: If the model cannot be loaded
    ///     ValueError: If the path contains invalid UTF-8
    #[new]
    #[pyo3(signature = (model: "Model | os.PathLike | str", n_ctx = 4096, system_prompt = "", allow_thinking = true, tools: "list[Tool]" = Vec::<Tool>::new(), sampler=SamplerConfig::default()) -> "Chat")]
    pub fn new(
        model: ModelOrPath,
        n_ctx: u32,
        system_prompt: &str,
        allow_thinking: bool,
        tools: Vec<Tool>,
        sampler: SamplerConfig,
    ) -> PyResult<Self> {
        let nw_model = model.get_inner_model()?;
        let chat_handle = nobodywho::chat::ChatBuilder::new(nw_model)
            .with_context_size(n_ctx)
            .with_tools(tools.into_iter().map(|t| t.tool).collect())
            .with_allow_thinking(allow_thinking)
            .with_system_prompt(system_prompt)
            .with_sampler(sampler.sampler_config)
            .build();
        Ok(Self { chat_handle })
    }

    /// Send a message to the model and get a streaming response.
    ///
    /// Args:
    ///     text: The user message to send
    ///
    /// Returns:
    ///     A TokenStream that yields tokens as they are generated
    pub fn ask(&self, text: String) -> TokenStream {
        TokenStream {
            stream: self.chat_handle.ask(text),
        }
    }

    /// Reset the conversation with a new system prompt and tools. Clears all chat history.
    ///
    /// Args:
    ///     system_prompt: New system message to guide the model's behavior
    ///     tools: New list of Tool instances the model can call
    ///
    /// Raises:
    ///     RuntimeError: If reset fails
    #[pyo3(signature = (system_prompt: "str", tools: "list[Tool]") -> "None")]
    pub fn reset(&self, system_prompt: String, tools: Vec<Tool>, py: Python) -> PyResult<()> {
        py.detach(|| {
            self.chat_handle
                .reset_chat(system_prompt, tools.into_iter().map(|t| t.tool).collect())
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }

    /// Clear the chat history while keeping the system prompt and tools unchanged.
    ///
    /// Raises:
    ///     RuntimeError: If reset fails
    #[pyo3(signature = () -> "None")]
    pub fn reset_history(&self, py: Python) -> PyResult<()> {
        py.detach(|| {
            self.chat_handle
                .reset_history()
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }

    /// Enable or disable extended reasoning tokens for supported models.
    ///
    /// Args:
    ///     allow_thinking: If True, allows extended reasoning tokens
    ///
    /// Raises:
    ///     ValueError: If the setting cannot be changed
    #[pyo3(signature = (allow_thinking: "bool") -> "None")]
    pub fn set_allow_thinking(&self, allow_thinking: bool, py: Python) -> PyResult<()> {
        py.detach(|| {
            self.chat_handle
                .set_allow_thinking(allow_thinking)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
        })
    }

    /// Get the current chat history as a list of message dictionaries.
    ///
    /// Returns:
    ///     List of message dicts, each with 'role' (str) and 'content' (str) keys.
    ///     Example: [{"role": "user", "content": "Hello"}, {"role": "assistant", "content": "Hi!"}]
    ///
    /// Raises:
    ///     RuntimeError: If retrieval fails
    #[pyo3(signature = () -> "list[dict]")]
    pub fn get_chat_history(&self, py: Python) -> PyResult<Py<PyAny>> {
        let msgs = py.detach(|| {
            self.chat_handle
                .get_chat_history()
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })?;

        pythonize::pythonize(py, &msgs)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
            .map(|bound| bound.unbind())
    }

    /// Replace the chat history with a new list of messages.
    ///
    /// Args:
    ///     msgs: List of message dicts, each with 'role' (str) and 'content' (str) keys.
    ///           Example: [{"role": "user", "content": "Hello"}, {"role": "assistant", "content": "Hi!"}]
    ///
    /// Raises:
    ///     ValueError: If message format is invalid
    ///     RuntimeError: If setting history fails
    #[pyo3(signature = (msgs: "list[dict]") -> "None")]
    pub fn set_chat_history(&self, msgs: Bound<'_, PyAny>, py: Python) -> PyResult<()> {
        let msgs = pythonize::depythonize(&msgs)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

        py.detach(|| {
            self.chat_handle
                .set_chat_history(msgs)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }
    /// Stop the current text generation immediately.
    ///
    /// This can be used to cancel an in-progress generation if the response is taking too long
    /// or is no longer needed.
    #[pyo3(signature = () -> "None")]
    pub fn stop_generation(&self, py: Python) {
        py.detach(|| self.chat_handle.stop_generation())
    }

    /// Update the list of tools available to the model without resetting chat history.
    ///
    /// Args:
    ///     tools: New list of Tool instances the model can call
    ///
    /// Raises:
    ///     RuntimeError: If updating tools fails
    #[pyo3(signature = (tools : "list[Tool]") -> "None")]
    pub fn set_tools(&self, tools: Vec<Tool>, py: Python) -> PyResult<()> {
        py.detach(|| {
            self.chat_handle
                .set_tools(tools.into_iter().map(|t| t.tool).collect())
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }

    /// Update the system prompt without resetting chat history.
    ///
    /// Args:
    ///     system_prompt: New system message to guide the model's behavior
    ///
    /// Raises:
    ///     RuntimeError: If the system prompt cannot be changed
    #[pyo3(signature = (system_prompt : "str") -> "None")]
    pub fn set_system_prompt(&self, system_prompt: String, py: Python) -> PyResult<()> {
        py.detach(|| {
            self.chat_handle
                .set_system_prompt(system_prompt)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }

    /// Update the sampler configuration without resetting chat history.
    ///
    /// Args:
    ///     sampler: New SamplerConfig for token selection
    ///
    /// Raises:
    ///     RuntimeError: If the sampler config cannot be changed
    #[pyo3(signature = (sampler : "SamplerConfig") -> "None")]
    pub fn set_sampler_config(&self, sampler: SamplerConfig, py: Python) -> PyResult<()> {
        py.detach(|| {
            self.chat_handle
                .set_sampler_config(sampler.sampler_config)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }
}

/// This is the async version of the `Chat` class.
/// See the docs for the `Chat` class for more information.
#[pyclass]
pub struct ChatAsync {
    chat_handle: nobodywho::chat::ChatHandleAsync,
}

#[pymethods]
impl ChatAsync {
    /// Create a new async Chat instance for conversational text generation.
    ///
    /// Args:
    ///     model: A chat model (Model instance or path to GGUF file)
    ///     n_ctx: Context size (maximum conversation length in tokens). Defaults to 4096.
    ///     system_prompt: System message to guide the model's behavior. Defaults to empty string.
    ///     allow_thinking: If True, allows extended reasoning tokens for supported models. Defaults to True.
    ///     tools: List of Tool instances the model can call. Defaults to empty list.
    ///     sampler: SamplerConfig for token selection. Defaults to SamplerConfig.default().
    ///
    /// Returns:
    ///     A ChatAsync instance
    ///
    /// Raises:
    ///     RuntimeError: If the model cannot be loaded
    ///     ValueError: If the path contains invalid UTF-8
    #[new]
    #[pyo3(signature = (model: "Model | os.PathLike | str", n_ctx = 4096, system_prompt = "", allow_thinking = true, tools: "list[Tool]" = vec![], sampler = SamplerConfig::default()) -> "ChatAsync")]
    pub fn new(
        model: ModelOrPath,
        n_ctx: u32,
        system_prompt: &str,
        allow_thinking: bool,
        tools: Vec<Tool>,
        sampler: SamplerConfig,
    ) -> PyResult<Self> {
        let nw_model = model.get_inner_model()?;
        let chat_handle = nobodywho::chat::ChatBuilder::new(nw_model)
            .with_context_size(n_ctx)
            .with_tools(tools.into_iter().map(|t| t.tool).collect())
            .with_allow_thinking(allow_thinking)
            .with_system_prompt(system_prompt)
            .with_sampler(sampler.sampler_config)
            .build_async();
        Ok(Self { chat_handle })
    }

    /// Send a message to the model and get a streaming response asynchronously.
    ///
    /// Args:
    ///     text: The user message to send
    ///
    /// Returns:
    ///     A TokenStreamAsync that yields tokens as they are generated
    pub fn ask(&self, text: String) -> TokenStreamAsync {
        TokenStreamAsync {
            stream: std::sync::Arc::new(tokio::sync::Mutex::new(self.chat_handle.ask(text))),
        }
    }

    /// Reset the conversation with a new system prompt and tools. Clears all chat history.
    ///
    /// Args:
    ///     system_prompt: New system message to guide the model's behavior
    ///     tools: New list of Tool instances the model can call
    ///
    /// Raises:
    ///     RuntimeError: If reset fails
    #[pyo3(signature = (system_prompt: "str", tools: "list[Tool]") -> "None")]
    pub async fn reset(&self, system_prompt: String, tools: Vec<Tool>) -> PyResult<()> {
        self.chat_handle
            .reset_chat(system_prompt, tools.into_iter().map(|t| t.tool).collect())
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Clear the chat history while keeping the system prompt and tools unchanged.
    ///
    /// Raises:
    ///     RuntimeError: If reset fails
    #[pyo3(signature = () -> "None")]
    pub async fn reset_history(&self) -> PyResult<()> {
        self.chat_handle
            .reset_history()
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Enable or disable extended reasoning tokens for supported models.
    ///
    /// Args:
    ///     allow_thinking: If True, allows extended reasoning tokens
    ///
    /// Raises:
    ///     ValueError: If the setting cannot be changed
    #[pyo3(signature = (allow_thinking: "bool") -> "None")]
    pub async fn set_allow_thinking(&self, allow_thinking: bool) -> PyResult<()> {
        self.chat_handle
            .set_allow_thinking(allow_thinking)
            .await
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Get the current chat history as a list of message dictionaries.
    ///
    /// Returns:
    ///     List of message dicts, each with 'role' (str) and 'content' (str) keys.
    ///     Example: [{"role": "user", "content": "Hello"}, {"role": "assistant", "content": "Hi!"}]
    ///
    /// Raises:
    ///     RuntimeError: If retrieval fails
    #[pyo3(signature = () -> "list[dict]")]
    pub async fn get_chat_history(&self) -> PyResult<Py<PyAny>> {
        let msgs = self
            .chat_handle
            .get_chat_history()
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        Python::attach(|py| {
            pythonize::pythonize(py, &msgs)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
                .map(|bound| bound.unbind())
        })
    }

    /// Replace the chat history with a new list of messages.
    ///
    /// Args:
    ///     msgs: List of message dicts, each with 'role' (str) and 'content' (str) keys.
    ///           Example: [{"role": "user", "content": "Hello"}, {"role": "assistant", "content": "Hi!"}]
    ///
    /// Raises:
    ///     ValueError: If message format is invalid
    ///     RuntimeError: If setting history fails
    #[pyo3(signature = (msgs: "list[dict]") -> "None")]
    pub async fn set_chat_history(&self, msgs: Py<PyAny>) -> PyResult<()> {
        let msgs = Python::attach(|py| {
            let bound_msgs = msgs.bind(py);
            pythonize::depythonize(bound_msgs)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
        })?;

        self.chat_handle
            .set_chat_history(msgs)
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Stop the current text generation immediately.
    ///
    /// This can be used to cancel an in-progress generation if the response is taking too long
    /// or is no longer needed.
    #[pyo3(signature = () -> "None")]
    pub async fn stop_generation(&self) {
        self.chat_handle.stop_generation()
    }

    /// Update the list of tools available to the model without resetting chat history.
    ///
    /// Args:
    ///     tools: New list of Tool instances the model can call
    ///
    /// Raises:
    ///     RuntimeError: If updating tools fails
    #[pyo3(signature = (tools : "list[Tool]") -> "None")]
    pub async fn set_tools(&self, tools: Vec<Tool>) -> PyResult<()> {
        self.chat_handle
            .set_tools(tools.into_iter().map(|t| t.tool).collect())
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Update the system prompt without resetting chat history.
    ///
    /// Args:
    ///     system_prompt: New system message to guide the model's behavior
    ///
    /// Raises:
    ///     RuntimeError: If the system prompt cannot be changed
    #[pyo3(signature = (system_prompt : "str") -> "None")]
    pub async fn set_system_prompt(&self, system_prompt: String) -> PyResult<()> {
        self.chat_handle
            .set_system_prompt(system_prompt)
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Update the sampler configuration without resetting chat history.
    ///
    /// Args:
    ///     sampler: New SamplerConfig for token selection
    ///
    /// Raises:
    ///     RuntimeError: If the sampler config cannot be changed
    #[pyo3(signature = (sampler : "SamplerConfig") -> "None")]
    pub async fn set_sampler_config(&self, sampler: SamplerConfig) -> PyResult<()> {
        self.chat_handle
            .set_sampler_config(sampler.sampler_config)
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }
}

/// Compute the cosine similarity between two vectors.
/// Particularly useful for comparing embedding vectors from an Encoder.
///
/// Args:
///     a: First vector
///     b: Second vector (must have the same length as a)
///
/// Returns:
///     Similarity score between 0.0 and 1.0 (higher means more similar)
///
/// Raises:
///     ValueError: If vectors have different lengths
#[pyfunction]
#[pyo3(signature = (a: "list[float]", b: "list[float]") -> "float")]
fn cosine_similarity(a: Vec<f32>, b: Vec<f32>) -> PyResult<f32> {
    if a.len() != b.len() {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "Vectors must have the same length",
        ));
    }
    Ok(nobodywho::encoder::cosine_similarity(&a, &b))
}

/// `SamplerConfig` contains the configuration for a token sampler. The mechanism by which
/// NobodyWho will sample a token from the probability distribution, to include in the
/// generation result.
/// A `SamplerConfig` can be constructed either using a preset function from the `SamplerPresets`
/// class, or by manually constructing a sampler chain using the `SamplerBuilder` class.
#[pyclass]
#[derive(Clone, Default)]
pub struct SamplerConfig {
    sampler_config: nobodywho::sampler_config::SamplerConfig,
}

/// `SamplerBuilder` is used to manually construct a sampler chain.
/// A sampler chain consists of any number of probability-shifting steps, and a single sampling step.
/// Probability-shifting steps are operations that transform the probability distribution of next
/// tokens, as generated by the model. E.g. the top_k step will zero the probability of all tokens
/// that aren't among the top K most probable (where K is some integer).
/// A sampling step is a final step that selects a single token from the probability distribution
/// that results from applying all of the probability-shifting steps in order.
/// E.g. the `dist` sampling step selects a token with weighted randomness, and the
/// `greedy` sampling step always selects the most probable.
#[pyclass]
#[derive(Clone)]
pub struct SamplerBuilder {
    sampler_config: nobodywho::sampler_config::SamplerConfig,
}

#[pymethods]
impl SamplerBuilder {
    /// Create a new SamplerBuilder to construct a custom sampler chain.
    #[new]
    #[pyo3(signature = () -> "SamplerBuilder")]
    pub fn new() -> Self {
        Self {
            sampler_config: nobodywho::sampler_config::SamplerConfig::default(),
        }
    }

    /// Keep only the top K most probable tokens. Typical values: 40-50.
    ///
    /// Args:
    ///     top_k: Number of top tokens to keep
    pub fn top_k(&self, top_k: i32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler_config::ShiftStep::TopK { top_k },
        )
    }

    /// Keep tokens whose cumulative probability is below top_p. Typical values: 0.9-0.95.
    ///
    /// Args:
    ///     top_p: Cumulative probability threshold (0.0 to 1.0)
    ///     min_keep: Minimum number of tokens to always keep
    pub fn top_p(&self, top_p: f32, min_keep: u32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler_config::ShiftStep::TopP { top_p, min_keep },
        )
    }

    /// Keep tokens with probability above min_p * (probability of most likely token).
    ///
    /// Args:
    ///     min_p: Minimum relative probability threshold (0.0 to 1.0). Typical: 0.05-0.1.
    ///     min_keep: Minimum number of tokens to always keep
    pub fn min_p(&self, min_p: f32, min_keep: u32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler_config::ShiftStep::MinP { min_p, min_keep },
        )
    }

    /// XTC (eXclude Top Choices) sampler that probabilistically excludes high-probability tokens.
    /// This can increase output diversity by sometimes forcing the model to pick less obvious tokens.
    ///
    /// Args:
    ///     xtc_probability: Probability of applying XTC on each token (0.0 to 1.0)
    ///     xtc_threshold: Tokens with probability above this threshold may be excluded (0.0 to 1.0)
    ///     min_keep: Minimum number of tokens to always keep (prevents excluding all tokens)
    pub fn xtc(&self, xtc_probability: f32, xtc_threshold: f32, min_keep: u32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler_config::ShiftStep::XTC {
                xtc_probability,
                xtc_threshold,
                min_keep,
            },
        )
    }

    /// Typical sampling: keeps tokens close to expected information content.
    ///
    /// Args:
    ///     typ_p: Typical probability mass (0.0 to 1.0). Typical: 0.9.
    ///     min_keep: Minimum number of tokens to always keep
    pub fn typical_p(&self, typ_p: f32, min_keep: u32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler_config::ShiftStep::TypicalP { typ_p, min_keep },
        )
    }

    /// Apply a grammar constraint to enforce structured output.
    ///
    /// Args:
    ///     grammar: Grammar specification in GBNF format (GGML BNF, a variant of BNF used by llama.cpp)
    ///     trigger_on: Optional string that, when generated, activates the grammar constraint.
    ///                 Useful for letting the model generate free-form text until a specific marker.
    ///     root: Name of the root grammar rule to start parsing from
    pub fn grammar(&self, grammar: String, trigger_on: Option<String>, root: String) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler_config::ShiftStep::Grammar {
                grammar,
                trigger_on,
                root,
            },
        )
    }

    /// DRY (Don't Repeat Yourself) sampler to reduce repetition.
    ///
    /// Args:
    ///     multiplier: Penalty strength multiplier
    ///     base: Base penalty value
    ///     allowed_length: Maximum allowed repetition length
    ///     penalty_last_n: Number of recent tokens to consider
    ///     seq_breakers: List of strings that break repetition sequences
    #[pyo3(signature = (multiplier: "float", base: "float", allowed_length: "int", penalty_last_n: "int", seq_breakers: "list[str]") -> "SamplerBuilder")]
    pub fn dry(
        &self,
        multiplier: f32,
        base: f32,
        allowed_length: i32,
        penalty_last_n: i32,
        seq_breakers: Vec<String>,
    ) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler_config::ShiftStep::DRY {
                multiplier,
                base,
                allowed_length,
                penalty_last_n,
                seq_breakers,
            },
        )
    }

    /// Apply repetition penalties to discourage repeated tokens.
    ///
    /// Args:
    ///     penalty_last_n: Number of recent tokens to penalize (0 = disable)
    ///     penalty_repeat: Base repetition penalty (1.0 = no penalty, >1.0 = penalize)
    ///     penalty_freq: Frequency penalty based on token occurrence count
    ///     penalty_present: Presence penalty for any token that appeared before
    pub fn penalties(
        &self,
        penalty_last_n: i32,
        penalty_repeat: f32,
        penalty_freq: f32,
        penalty_present: f32,
    ) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler_config::ShiftStep::Penalties {
                penalty_last_n,
                penalty_repeat,
                penalty_freq,
                penalty_present,
            },
        )
    }

    /// Apply temperature scaling to the probability distribution.
    ///
    /// Args:
    ///     temperature: Temperature value (0.0 = deterministic, 1.0 = unchanged, >1.0 = more random)
    pub fn temperature(&self, temperature: f32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler_config::ShiftStep::Temperature { temperature },
        )
    }

    /// Sample from the probability distribution (weighted random selection).
    ///
    /// Returns:
    ///     A complete SamplerConfig ready to use
    pub fn dist(&self) -> SamplerConfig {
        sample_step(self.clone(), nobodywho::sampler_config::SampleStep::Dist)
    }

    /// Always select the most probable token (deterministic).
    ///
    /// Returns:
    ///     A complete SamplerConfig ready to use
    pub fn greedy(&self) -> SamplerConfig {
        sample_step(self.clone(), nobodywho::sampler_config::SampleStep::Greedy)
    }

    /// Use Mirostat v1 algorithm for perplexity-controlled sampling.
    /// Mirostat dynamically adjusts sampling to maintain a target "surprise" level,
    /// producing more coherent output than fixed temperature. Good for long-form generation.
    ///
    /// Args:
    ///     tau: Target perplexity/surprise value (typically 3.0-5.0; lower = more focused)
    ///     eta: Learning rate for perplexity adjustment (typically 0.1)
    ///     m: Number of candidates to consider (typically 100)
    ///
    /// Returns:
    ///     A complete SamplerConfig ready to use
    pub fn mirostat_v1(&self, tau: f32, eta: f32, m: i32) -> SamplerConfig {
        sample_step(
            self.clone(),
            nobodywho::sampler_config::SampleStep::MirostatV1 { tau, eta, m },
        )
    }

    /// Use Mirostat v2 algorithm for perplexity-controlled sampling.
    /// Mirostat v2 is a simplified version of Mirostat that's often preferred.
    /// It dynamically adjusts sampling to maintain a target "surprise" level.
    ///
    /// Args:
    ///     tau: Target perplexity/surprise value (typically 3.0-5.0; lower = more focused)
    ///     eta: Learning rate for perplexity adjustment (typically 0.1)
    ///
    /// Returns:
    ///     A complete SamplerConfig ready to use
    pub fn mirostat_v2(&self, tau: f32, eta: f32) -> SamplerConfig {
        sample_step(
            self.clone(),
            nobodywho::sampler_config::SampleStep::MirostatV2 { tau, eta },
        )
    }
}

fn shift_step(
    builder: SamplerBuilder,
    step: nobodywho::sampler_config::ShiftStep,
) -> SamplerBuilder {
    SamplerBuilder {
        sampler_config: builder.sampler_config.shift(step),
    }
}

fn sample_step(
    builder: SamplerBuilder,
    step: nobodywho::sampler_config::SampleStep,
) -> SamplerConfig {
    SamplerConfig {
        sampler_config: builder.sampler_config.sample(step),
    }
}

/// `SamplerPresets` is a static class which contains a bunch of functions to easily create a
/// `SamplerConfig` from some pre-defined sampler chain.
/// E.g. `SamplerPresets.temperature(0.8)` will return a `SamplerConfig` with temperature=0.8.
#[pyclass]
pub struct SamplerPresets {}

#[pymethods]
impl SamplerPresets {
    /// Get the default sampler configuration.
    #[staticmethod]
    pub fn default() -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler_config::SamplerConfig::default(),
        }
    }

    /// Create a sampler with top-k filtering only.
    ///
    /// Args:
    ///     top_k: Number of top tokens to keep
    #[staticmethod]
    pub fn top_k(top_k: i32) -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler_config::SamplerPresets::top_k(top_k),
        }
    }

    /// Create a sampler with nucleus (top-p) sampling.
    ///
    /// Args:
    ///     top_p: Cumulative probability threshold (0.0 to 1.0)
    #[staticmethod]
    pub fn top_p(top_p: f32) -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler_config::SamplerPresets::top_p(top_p),
        }
    }

    /// Create a greedy sampler (always picks most probable token).
    #[staticmethod]
    pub fn greedy() -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler_config::SamplerPresets::greedy(),
        }
    }

    /// Create a sampler with temperature scaling.
    ///
    /// Args:
    ///     temperature: Temperature value (lower = more focused, higher = more random)
    #[staticmethod]
    pub fn temperature(temperature: f32) -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler_config::SamplerPresets::temperature(temperature),
        }
    }

    /// Create a DRY sampler preset to reduce repetition.
    #[staticmethod]
    pub fn dry() -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler_config::SamplerPresets::dry(),
        }
    }

    /// Create a sampler configured for JSON output generation.
    /// Uses a grammar constraint to ensure the model outputs only valid JSON.
    #[staticmethod]
    pub fn json() -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler_config::SamplerPresets::json(),
        }
    }

    /// Create a sampler with a custom grammar constraint.
    ///
    /// Args:
    ///     grammar: Grammar specification in GBNF format (GGML BNF, a variant of BNF used by llama.cpp)
    #[staticmethod]
    pub fn grammar(grammar: String) -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler_config::SamplerPresets::grammar(grammar),
        }
    }
}

/// A `Tool` is a wrapped python function, that can be passed as a tool for the model to call.
/// `Tool`s are constructed using the `@tool` decorator.
#[pyclass]
pub struct Tool {
    tool: nobodywho::chat::Tool,
    pyfunc: Py<PyAny>,
}

impl Clone for Tool {
    fn clone(&self) -> Self {
        Python::attach(|py| Self {
            tool: self.tool.clone(),
            pyfunc: self.pyfunc.clone_ref(py),
        })
    }
}

#[pymethods]
impl Tool {
    #[pyo3(signature = (*args, **kwargs) -> "T")]
    fn __call__(
        &self,
        args: &Bound<pyo3::types::PyTuple>,
        kwargs: Option<&Bound<pyo3::types::PyDict>>,
        py: Python,
    ) -> PyResult<Py<PyAny>> {
        self.pyfunc.call(py, args, kwargs)
    }
}

/// Decorator to convert a Python function into a Chat-compatible Tool instance.
///
/// The decorated function will be callable by the model during chat. The model sees the
/// function's name, description, and parameter types/descriptions to decide when to call it.
///
/// Both synchronous and asynchronous functions are supported. Async functions are executed
/// synchronously when called by the model.
///
/// Args:
///     description: A description of what the tool does (shown to the model)
///     params: Optional dict mapping parameter names to their descriptions (shown to the model)
///
/// Returns:
///     A decorator that transforms a function into a Tool instance
///
/// Examples:
///     @tool("Get the current weather for a city", params={"city": "The city name"})
///     def get_weather(city: str) -> str:
///         return f"Weather in {city}: sunny"
///
///     @tool("Fetch data from a URL", params={"url": "The URL to fetch"})
///     async def fetch_url(url: str) -> str:
///         import aiohttp
///         async with aiohttp.ClientSession() as session:
///             async with session.get(url) as response:
///                 return await response.text()
///
/// Note:
///     All function parameters must have type hints. The function should return a string.
///     Async functions (defined with 'async def') are automatically detected and handled.
#[pyfunction(signature = (description: "str", params: "dict[str, str] | None" = None) -> "typing.Callable[[typing.Callable[..., T]], Tool]")]
fn tool<'a>(
    description: String,
    params: Option<Py<pyo3::types::PyDict>>,
    py: Python<'a>,
) -> PyResult<Bound<'a, pyo3::types::PyCFunction>> {
    // extract hashmap from parameter descriptions, default to empty hashmap
    let params: std::collections::HashMap<String, String> = match params {
        Some(pd) => pd.extract(py)?,
        None => std::collections::HashMap::new(),
    };

    // the decorator returned when calling @tool(...)
    // a function that takes the native-python function and returns a callable `Tool` object
    let function_to_tool = move |args: &Bound<pyo3::types::PyTuple>,
                                 _kwargs: Option<&Bound<pyo3::types::PyDict>>|
          -> PyResult<Tool> {
        Python::attach(|py| {
            // extract the function from *args
            let fun: Py<PyAny> = args.get_item(0)?.extract()?;

            // get the name of the function
            let name = fun.getattr(py, "__name__")?.extract::<String>(py)?;

            // detect if function is async
            let inspect = PyModule::import(py, "inspect")?;
            let is_async = inspect
                .getattr("iscoroutinefunction")?
                .call1((&fun,))?
                .extract::<bool>()?;

            // generate json schema from function type annotations
            let json_schema = python_func_json_schema(py, &fun, &params)?;

            let fun_clone = fun.clone_ref(py);

            // wrap the passed function in a json -> String function
            let wrapped_function = move |json: serde_json::Value| {
                Python::attach(|py| {
                    // construct kwargs to call the function with
                    let kwargs = match json_to_kwargs(py, json) {
                        Ok(kwargs) => kwargs,
                        Err(e) => return format!("ERROR: Failed to convert arguments: {e}"),
                    };

                    let py_result = if is_async {
                        let coroutine = match fun.call(py, (), Some(&kwargs)) {
                            Ok(coro) => coro,
                            Err(e) => return format!("ERROR: {e}"),
                        };

                        // Use Python's asyncio.run() to execute the coroutine
                        let asyncio = match py.import("asyncio") {
                            Ok(module) => module,
                            Err(e) => return format!("ERROR: Failed to import asyncio: {e}"),
                        };

                        asyncio.call_method1("run", (coroutine,)).map(|r| r.into())
                    } else {
                        fun.call(py, (), Some(&kwargs))
                    };

                    // extract a string from the result
                    // return an error string to the LLM if anything fails
                    match py_result.and_then(|r| r.extract::<String>(py)) {
                        Ok(s) => s,
                        Err(e) => format!("ERROR: {e}"),
                    }
                })
            };

            let tool = nobodywho::chat::Tool::new(
                name,
                description.clone(),
                json_schema,
                std::sync::Arc::new(wrapped_function),
            );

            Ok(Tool {
                tool,
                pyfunc: fun_clone,
            })
        })
    };

    pyo3::types::PyCFunction::new_closure(py, None, None, function_to_tool)
}

// takes a python function (assumes static types), and returns a json schema for that function
fn python_func_json_schema(
    py: Python,
    fun: &Py<PyAny>,
    param_descriptions: &std::collections::HashMap<String, String>,
) -> PyResult<serde_json::Value> {
    // import inspect (from stdlib)
    let inspect = PyModule::import(py, "inspect")?;

    // call `inspect.getfullargspec`
    // (not sure when getfullargspec was first added- but it *is* in 3.4 and later)
    let getfullargspec = inspect.getattr("getfullargspec")?;
    let argspec = getfullargspec.call((fun,), None)?;
    let annotations = argspec
        .getattr("annotations")?
        .extract::<std::collections::HashMap<String, Bound<pyo3::types::PyType>>>()?;
    let args = argspec.getattr("args")?.extract::<Vec<String>>()?;

    // check that all arguments are annotated
    if let Some(missing_arg) = args.iter().find(|arg| !annotations.contains_key(*arg)) {
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "ERROR: Parameter '{missing_arg}' is missing a type hint. NobodyWho requires all tool function parameters to have static type hints. E.g.: `{missing_arg}: str`"
        )));
    }

    // check that return type is `str`
    // the intent of this is to force people to consider how to convert to string
    if annotations
        .get("return")
        .map(|t| t.name().map(|n| n.to_string()))
        .transpose()?
        != Some("str".to_string())
    {
        tracing::warn!("Return type of this tool should be `str`. Anything else will be cast to string, which might lead to unexpected results. It's recommended that you add a return type annotation to the tool: `-> str:`");
    }

    // check that names of parameter descriptions correspond to names of actual function arguments
    if let Some(invalid_param) = param_descriptions
        .keys()
        .find(|param| !args.contains(param))
    {
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "ERROR: Parameter description provided for '{invalid_param}' but function has no such parameter. Available parameters: [{}]",
            args.join(", ")
        )));
    }

    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for (key, value) in annotations {
        if key == "return" {
            continue;
        }

        let type_name = value.name()?.to_string();

        let schema_type = match type_name.as_str() {
            "str" => "string",
            "int" => "integer",
            "float" => "number",
            "bool" => "boolean",
            "list" => "array",
            "dict" => "object",
            "None" | "NoneType" => "null",
            // TODO: we could consider supporting sets like this:
            // "set" | "frozenset" => serde_json::json!({"type": "array", "uniqueItems": true}),
            // TODO: consider handling pydantic types?
            //       (objects subclassing pydantic's BaseModel can readily generate json schemas)
            // TODO: handle generic types better. at least handle list[int], dict[str,int], etc.
            _ if type_name.starts_with("list[") => "array",
            _ if type_name.starts_with("dict[") => "object",
            _ if type_name == "List" => "array",
            _ if type_name == "Dict" => "object",
            _ => {
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "ERROR: Tool function contains an unsupported type hint: {type_name}"
                )));
            }
        };

        let property = if let Some(description) = param_descriptions.get(&key) {
            // add description if available
            serde_json::json!({
                "type": schema_type,
                "description": description
            })
        } else {
            // ...otherwise only use the type
            serde_json::json!({
                "type": schema_type
            })
        };

        // add to json schema properties
        properties.insert(key.clone(), property);

        // add to list of required keys for object
        // TODO: allow optional parameters for params that have a default argument
        required.push(key);
    }

    // assemble the complete json schema for an arguments object
    let kwargs_schema = serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required
    });

    Ok(kwargs_schema)
}

// takes a sede_json::value, assumed to be an object, and returns a PyDict
fn json_to_kwargs(py: Python, json: serde_json::Value) -> PyResult<Bound<pyo3::types::PyDict>> {
    let py_dict = pyo3::types::PyDict::new(py);

    match json {
        serde_json::Value::Object(obj) => {
            for (k, v) in obj {
                let value_py = json_value_to_py(py, &v)?;
                py_dict.set_item(k, value_py)?;
            }
            Ok(py_dict)
        }
        _ =>
        // it's not an object. fail hard.
        // this branch should be impossible to hit.
        {
            Err(pyo3::exceptions::PyValueError::new_err(
                "Tool was passed some json that wasn't an object. It must be an object.",
            ))
        }
    }
}

// Helper function to convert serde_json::Value to PyObject
fn json_value_to_py<'py>(py: Python<'py>, value: &serde_json::Value) -> PyResult<Py<PyAny>> {
    match value {
        serde_json::Value::Null => Ok(py.None()),
        serde_json::Value::Bool(b) => Ok(pyo3::types::PyBool::new(py, *b).to_owned().into()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i128() {
                Ok(pyo3::types::PyInt::new(py, i).into())
            } else if let Some(i) = n.as_u128() {
                Ok(pyo3::types::PyInt::new(py, i).into())
            } else if let Some(f) = n.as_f64() {
                Ok(pyo3::types::PyFloat::new(py, f).into())
            } else {
                Err(pyo3::exceptions::PyValueError::new_err("Invalid number"))
            }
        }
        serde_json::Value::String(s) => Ok(pyo3::types::PyString::new(py, s).into()),
        serde_json::Value::Array(arr) => {
            let py_items: PyResult<Vec<_>> = arr.iter().map(|v| json_value_to_py(py, v)).collect();
            let pylist = pyo3::types::PyList::new(py, py_items?);
            match pylist {
                Ok(list) => Ok(list.into()),
                Err(_) => Err(pyo3::exceptions::PyValueError::new_err("Invalid number")),
            }
        }
        serde_json::Value::Object(obj) => {
            let py_dict = pyo3::types::PyDict::new(py);
            for (k, v) in obj {
                let value_py = json_value_to_py(py, v)?;
                py_dict.set_item(k, value_py)?;
            }
            Ok(py_dict.into())
        }
    }
}

#[pymodule(name = "nobodywho")]
pub mod nobodywhopython {
    use pyo3::prelude::*;

    #[pymodule_init]
    fn init(_m: &Bound<'_, PyModule>) -> PyResult<()> {
        // collect llamacpp logs in tracing
        // this will send llamacpp logs into `tracing`
        nobodywho::send_llamacpp_logs_to_tracing();

        // init the rust->python logging bridge
        // this will pick up logs from rust's `log`, and send those into python's `logging`
        // the "log" feature in the `tracing` crate will send tracing logs to the `log` interface
        // so: rust's `tracing` -> rust's `log` -> python's `logging`
        // this works as long as no other tracing_subscriber is active. otherwise we'd need `"log-always"`
        pyo3_log::init();
        Ok(())
    }

    #[pymodule_export]
    use super::cosine_similarity;
    #[pymodule_export]
    use super::tool;
    #[pymodule_export]
    use super::Chat;
    #[pymodule_export]
    use super::ChatAsync;
    #[pymodule_export]
    use super::CrossEncoder;
    #[pymodule_export]
    use super::CrossEncoderAsync;
    #[pymodule_export]
    use super::Encoder;
    #[pymodule_export]
    use super::EncoderAsync;
    #[pymodule_export]
    use super::Model;
    #[pymodule_export]
    use super::SamplerBuilder;
    #[pymodule_export]
    use super::SamplerConfig;
    #[pymodule_export]
    use super::SamplerPresets;
    #[pymodule_export]
    use super::TokenStream;
    #[pymodule_export]
    use super::TokenStreamAsync;
    #[pymodule_export]
    use super::Tool;
}
