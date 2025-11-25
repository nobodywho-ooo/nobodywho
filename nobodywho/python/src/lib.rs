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
#[derive(Clone)]
pub struct Tool {
    tool: nobodywho::chat::Tool,
}

#[pymethods]
impl Tool {
    #[new]
    #[pyo3(signature = (fun, description))]
    pub fn new(fun: Py<PyAny>, description: String, py: Python) -> PyResult<Self> {
        // get the name of the function
        let name = fun.getattr(py, "__name__")?.extract::<String>(py)?;

        let description = description;

        // TODO: get types of python function
        let json_schema = python_func_json_schema(py, &fun)?;

        // wrap the passed function in a json -> String function
        let function = move |json: serde_json::Value| {
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
            description,
            json_schema,
            std::sync::Arc::new(function),
        );

        Ok(Self { tool })
    }
}

// takes a python function (assumes static types), and returns a json schema for that function
fn python_func_json_schema(py: Python, fun: &Py<PyAny>) -> PyResult<serde_json::Value> {
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
    for arg in args {
        if !annotations.contains_key(&arg) {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "ERROR: NobodyWho requires all tool function parameters to be typed. Parameter {arg} is missing a type hint. Please add a static type hint to that parameter. For example: `{arg}: int`"
            )));
        }
    }

    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for (key, value) in annotations {
        if key == "return" {
            continue;
        }

        let type_name = value.name()?.to_string();
        println!("{key} - {:?}", type_name);

        let schema_type = match type_name.as_str() {
            "str" => serde_json::json!({"type": "string"}),
            "int" => serde_json::json!({"type": "integer"}),
            "float" => serde_json::json!({"type": "number"}),
            "bool" => serde_json::json!({"type": "boolean"}),
            "list" => serde_json::json!({"type": "array"}),
            "dict" => serde_json::json!({"type": "object"}),
            "None" | "NoneType" => serde_json::json!({"type": "null"}),
            "Any" | "any" => serde_json::json!({}), // Allow any type
            // TODO: these two are kinda bad.. they will be passed to the function as lists
            "set" | "frozenset" => serde_json::json!({"type": "array", "uniqueItems": true}),
            "tuple" => serde_json::json!({"type": "array"}),
            // TODO: bytes? - "bytes" | "bytearray" => todo!(),
            // TODO: handle generic types better. at least handle list[int], dict[str,int], etc.
            // TODO: consider handling more complex built-in things? Union, Option, `|`,
            // TODO: consider handling pydantic types?
            //       (objects subclassing pydantic's BaseModel can readily generate json schemas)
            _ if type_name.starts_with("list[") => serde_json::json!({"type": "array"}),
            _ if type_name.starts_with("dict[") => serde_json::json!({"type": "object"}),
            _ if type_name == "List" => serde_json::json!({"type": "array"}),
            _ if type_name == "Dict" => serde_json::json!({"type": "object"}),
            _ => {
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "ERROR: Tool function contains an unsupported type hint: {type_name}"
                )));
            }
        };

        // add to json schema properties
        properties.insert(key.clone(), schema_type);

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
    m.add_class::<TokenStream>()?;
    m.add_class::<Embeddings>()?;
    m.add_class::<CrossEncoder>()?;
    m.add_function(wrap_pyfunction!(cosine_similarity, m)?)?;
    m.add_class::<Tool>()?;
    Ok(())
}
