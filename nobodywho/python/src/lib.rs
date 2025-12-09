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
    pub fn next_token(&mut self, py: Python) -> Option<String> {
        // Release the GIL while waiting for the next token
        // This allows the background thread to acquire the GIL if needed for tool calls
        py.detach(|| self.stream.next_token())
    }

    pub fn completed(&mut self, py: Python) -> String {
        py.detach(|| self.stream.completed())
    }

    // sync iterator stuff
    pub fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    // XXX TODO: does iterators still work?
    pub fn __next__(&mut self, py: Python) -> Option<String> {
        py.detach(|| self.stream.next_token())
    }

    // TODO: async iterator (turns out to be trickier than expected)
}

#[pyclass]
pub struct TokenStreamAsync {
    stream: nobodywho::chat::TokenStreamAsync,
}

#[pymethods]
impl TokenStreamAsync {
    pub async fn next_token(&mut self) -> Option<String> {
        // no need to release GIL in async functions
        self.stream.next_token().await
    }

    pub async fn completed(&mut self) -> String {
        self.stream.completed().await
    }

    // TODO: async iterator (turns out to be trickier than expected)
}

#[pyclass]
pub struct Encoder {
    encoder: nobodywho::encoder::Encoder,
}

#[pymethods]
impl Encoder {
    #[new]
    #[pyo3(signature = (model, n_ctx = 2048))]
    pub fn new(model: &Model, n_ctx: u32) -> Self {
        let encoder = nobodywho::encoder::Encoder::new(model.model.clone(), n_ctx);
        Self { encoder }
    }

    pub fn encode(&self, text: String, py: Python) -> PyResult<Vec<f32>> {
        py.detach(|| {
            self.encoder
                .encode(text)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))
        })
    }
}

#[pyclass]
pub struct EncoderAsync {
    encoder_handle: nobodywho::encoder::EncoderAsync,
}

#[pymethods]
impl EncoderAsync {
    #[new]
    #[pyo3(signature = (model, n_ctx = 2048))]
    pub fn new(model: &Model, n_ctx: u32) -> Self {
        let encoder_handle = nobodywho::encoder::EncoderAsync::new(model.model.clone(), n_ctx);
        Self { encoder_handle }
    }

    async fn encode(&self, text: String) -> PyResult<Vec<f32>> {
        let mut rx = self.encoder_handle.encode(text);
        rx.recv().await.ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Failed to receive embedding")
        })
    }
}

#[pyclass]
pub struct CrossEncoder {
    crossencoder: nobodywho::crossencoder::CrossEncoder,
}

#[pymethods]
impl CrossEncoder {
    #[new]
    #[pyo3(signature = (model, n_ctx = 2048))]
    pub fn new(model: &Model, n_ctx: u32) -> Self {
        let crossencoder = nobodywho::crossencoder::CrossEncoder::new(model.model.clone(), n_ctx);
        Self { crossencoder }
    }

    pub fn rank(&self, query: String, documents: Vec<String>, py: Python) -> PyResult<Vec<f32>> {
        py.detach(|| {
            self.crossencoder
                .rank(query, documents)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))
        })
    }

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

#[pyclass]
pub struct CrossEncoderAsync {
    crossencoder_handle: nobodywho::crossencoder::CrossEncoderAsync,
}

#[pymethods]
impl CrossEncoderAsync {
    #[new]
    #[pyo3(signature = (model, n_ctx = 2048))]
    pub fn new(model: &Model, n_ctx: u32) -> Self {
        let crossencoder_handle =
            nobodywho::crossencoder::CrossEncoderAsync::new(model.model.clone(), n_ctx);
        Self {
            crossencoder_handle,
        }
    }

    async fn rank(&self, query: String, documents: Vec<String>) -> PyResult<Vec<f32>> {
        let mut rx = self.crossencoder_handle.rank(query, documents);
        rx.recv().await.ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Failed to receive ranking scores")
        })
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
    #[pyo3(signature = (model, n_ctx = 2048, system_prompt = "", allow_thinking = true, tools = vec![]))]
    pub fn new(
        model: &Model,
        n_ctx: u32,
        system_prompt: &str,
        allow_thinking: bool,
        tools: Vec<Tool>,
    ) -> Self {
        let chat_handle = nobodywho::chat::ChatBuilder::new(model.model.clone())
            .with_context_size(n_ctx)
            .with_tools(tools.into_iter().map(|t| t.tool).collect())
            .with_allow_thinking(allow_thinking)
            .with_system_prompt(system_prompt)
            .build();
        Self { chat_handle }
    }

    pub fn ask(&self, text: String) -> TokenStream {
        TokenStream {
            stream: self.chat_handle.ask(text),
        }
    }
}

#[pyclass]
pub struct ChatAsync {
    chat_handle: nobodywho::chat::ChatHandleAsync,
}

#[pymethods]
impl ChatAsync {
    #[new]
    #[pyo3(signature = (model, n_ctx = 2048, system_prompt = "", allow_thinking = true, tools = vec![]))]
    pub fn new(
        model: &Model,
        n_ctx: u32,
        system_prompt: &str,
        allow_thinking: bool,
        tools: Vec<Tool>,
    ) -> Self {
        let chat_handle = nobodywho::chat::ChatBuilder::new(model.model.clone())
            .with_context_size(n_ctx)
            .with_tools(tools.into_iter().map(|t| t.tool).collect())
            .with_allow_thinking(allow_thinking)
            .with_system_prompt(system_prompt)
            .build_async();
        Self { chat_handle }
    }

    pub fn ask(&self, text: String) -> TokenStreamAsync {
        TokenStreamAsync {
            stream: self.chat_handle.ask(text),
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
    Ok(nobodywho::encoder::cosine_similarity(&a, &b))
}

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
    #[pyo3(signature = (*args, **kwargs))]
    fn __call__(
        &self,
        args: &Bound<pyo3::types::PyTuple>,
        kwargs: Option<&Bound<pyo3::types::PyDict>>,
        py: Python,
    ) -> PyResult<Py<PyAny>> {
        self.pyfunc.call(py, args, kwargs)
    }
}

// tool decorator
#[pyfunction(signature = (description, params=None))]
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

#[pymodule]
#[pyo3(name = "nobodywho")]
fn nobodywhopython(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Model>()?;
    m.add_class::<Chat>()?;
    m.add_class::<ChatAsync>()?;
    m.add_class::<TokenStream>()?;
    m.add_class::<TokenStreamAsync>()?;
    m.add_class::<Encoder>()?;
    m.add_class::<EncoderAsync>()?;
    m.add_class::<CrossEncoder>()?;
    m.add_class::<CrossEncoderAsync>()?;
    m.add_function(wrap_pyfunction!(cosine_similarity, m)?)?;
    m.add_function(wrap_pyfunction!(tool, m)?)?;
    m.add_class::<Tool>()?;
    Ok(())
}
