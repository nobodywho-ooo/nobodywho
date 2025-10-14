use nobodywho::{
    chat::{self, ChatBuilder, TokenStream, Tool},
    llm,
    sampler_config::SamplerConfig,
};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use pyo3_async_runtimes::tokio::{future_into_py, get_runtime};
use serde_json;
use std::sync::Arc;

#[pyclass]
pub struct NobodyWhoModel {
    use_gpu_if_available: bool,
    model: Option<llm::Model>,
}

#[pymethods]
impl NobodyWhoModel {
    #[new]
    #[pyo3(signature = (model_path, use_gpu_if_available = true))]
    pub fn new(model_path: &str, use_gpu_if_available: bool) -> Self {
        Self {
            use_gpu_if_available: use_gpu_if_available,
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

/// Python wrapper for a tool that can be called by the model
#[pyclass]
#[derive(FromPyObject)]
pub struct NobodyWhoTool {
    name: String,
    description: String,
    parameters: Py<PyDict>,
    callback: Py<PyAny>,
}

#[pymethods]
impl NobodyWhoTool {
    #[new]
    pub fn new(
        name: String,
        description: String,
        parameters: Py<PyDict>,
        callback: Py<PyAny>,
    ) -> Self {
        Self {
            name,
            description,
            parameters,
            callback,
        }
    }
}

impl NobodyWhoTool {
    /// Convert Python dict parameters to JSON schema
    fn parameters_to_json(&self, py: Python) -> PyResult<serde_json::Value> {
        let dict = self.parameters.bind(py);
        let json_str = py
            .import("json")?
            .getattr("dumps")?
            .call1((dict,))?
            .extract::<String>()?;

        serde_json::from_str(&json_str).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Failed to parse parameters as JSON: {}",
                e
            ))
        })
    }

    /// Convert this Python tool to a Rust Tool
    fn to_rust_tool(&self, py: Python) -> PyResult<Tool> {
        let json_schema = self.parameters_to_json(py)?;
        let callback = self.callback.clone_ref(py);

        let function = Arc::new(move |args: serde_json::Value| -> String {
            // We need to call back into Python, so we need the GIL
            Python::attach(|py| {
                // Convert JSON arguments to Python dict
                let json_str = args.to_string();
                let py_dict = match py
                    .import("json")
                    .and_then(|json_mod| json_mod.getattr("loads"))
                    .and_then(|loads| loads.call1((json_str,)))
                {
                    Ok(dict) => dict,
                    Err(e) => return format!("Error parsing arguments: {}", e),
                };

                // Call the Python function
                match callback.bind(py).call1((py_dict,)) {
                    Ok(result) => {
                        // Try to extract string result
                        match result.extract::<String>() {
                            Ok(s) => s,
                            Err(_) => {
                                // If not a string, try to convert using str()
                                match result.str() {
                                    Ok(py_str) => py_str.to_string(),
                                    Err(e) => format!("Error converting result to string: {}", e),
                                }
                            }
                        }
                    }
                    Err(e) => format!("Error calling tool function: {}", e),
                }
            })
        });

        Ok(Tool::new(
            self.name.clone(),
            self.description.clone(),
            json_schema,
            function,
        ))
    }
}

#[pyclass]
pub struct NobodyWhoChat {
    chat_handle: Option<chat::ChatHandle>,
}

#[pymethods]
impl NobodyWhoChat {
    #[new]
    #[pyo3(signature = (model, n_ctx = 2048, system_prompt = "", tools = vec![]))]
    pub fn new(
        model: &NobodyWhoModel,
        n_ctx: u32,
        system_prompt: &str,
        tools: Vec<NobodyWhoTool>,
        py: Python,
    ) -> PyResult<Self> {
        let mut rust_tools = vec![];

        for pytool in tools {
            let r_tool = match pytool.to_rust_tool(py) {
                Ok(t) => t,
                _ => {
                    return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                        "Could not build tools!",
                    ))
                }
            };
            rust_tools.push(r_tool);
        }

        if let Some(ref model) = model.model {
            let cb = ChatBuilder::new(Arc::clone(model));
            Ok(Self {
                chat_handle: Some(
                    cb.with_context_size(n_ctx)
                        .with_system_prompt(system_prompt)
                        .with_tools(rust_tools)
                        .build(),
                ),
            })
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Model is not initialized",
            ))
        }
    }

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
    m.add_class::<NobodyWhoChat>()?;
    m.add_class::<NobodyWhoTokenStream>()?;
    Ok(())
}
