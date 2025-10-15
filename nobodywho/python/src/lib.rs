use nobodywho::{
    chat::{self, ChatBuilder, TokenStream, Tool, ToolBuilder},
    llm,
};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use serde_json;
use std::sync::Arc;
use tracing::debug;

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

/// Python wrapper for a tool that can be called by the model
#[pyclass]
#[derive(FromPyObject)]
pub struct NobodyWhoTool {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    description: String,
    #[pyo3(get)]
    parameters: Vec<(String, String, String)>,
    #[pyo3(get)]
    callback: Py<PyAny>,
}

#[pymethods]
impl NobodyWhoTool {
    #[new]
    pub fn new(
        name: String,
        description: String,
        parameters: Vec<(String, String, String)>,
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
    fn to_rust_tool(&self, py: Python) -> Tool {
        debug!("Attempting to create tool!");

        let callback = self.callback.clone_ref(py);
        let function = move |args: serde_json::Value| -> String {
            // We need to call back into Python, so we need the GIL
            debug!("Entering tool call closure!");
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

                debug!("Inside tool call closure");
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
        };

        let mut tool_builder = ToolBuilder::new(self.name.clone())
            .description(self.description.clone())
            .handler(function);

        for (name, param_type, description) in &self.parameters {
            tool_builder = tool_builder.param(name, param_type, description)
        }
        debug!("Tool built!");
        return tool_builder.build();
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
        // !!!!!!!!!!!!!!!!!!!!!!!!!!! THIS TRACING CODE SHOULD ONLY BE RUN ONCE AND REMOVED BEFORE EVEN CLOSE TO PRODUCTION !!!!!!!!!!!!!!!!!!
        // tracing_subscriber::fmt()
        //     .with_max_level(tracing::Level::TRACE)
        //     .with_timer(tracing_subscriber::fmt::time::uptime())
        //     .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        //     .try_init()
        //     .ok();
        let mut rust_tools = vec![];

        for pytool in tools {
            let r_tool = pytool.to_rust_tool(py);
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

#[pyfunction]
pub fn function_test_call(callback: Py<PyAny>, args: Bound<'_, PyDict>) -> PyResult<Py<PyAny>> {
    match Python::attach(|py| {
        let tup = PyTuple::new(py, &[args])?;

        callback.call1(py, tup)
    }) {
        Ok(py_obj) => Ok(py_obj),
        Err(e) => Err(e),
    }
}

#[pymodule]
fn python(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<NobodyWhoModel>()?;
    m.add_class::<NobodyWhoChat>()?;
    m.add_class::<NobodyWhoTokenStream>()?;
    m.add_class::<NobodyWhoTool>()?;
    m.add_function(wrap_pyfunction!(function_test_call, m)?)?;
    Ok(())
}
