use interoptopus::patterns::result::FFIError;
use interoptopus::patterns::slice::FFISlice;
use interoptopus::patterns::string::AsciiPointer;
use interoptopus::{
    callback, ffi_function, ffi_service, ffi_service_ctor, ffi_service_method, ffi_type, function,
    pattern, Inventory, InventoryBuilder,
};
use nobodywho::sampler_config::{SamplerConfig, SamplerPresets};
use std::ffi::c_char;
use std::sync::Arc;
use tracing::{debug, error, warn};

/// TRACING
static INIT: std::sync::Once = std::sync::Once::new();

/// Initialize tracing for tests
#[ffi_function]
#[no_mangle]
pub extern "C" fn init_tracing() {
    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_timer(tracing_subscriber::fmt::time::uptime())
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
            .try_init()
            .ok();
    });
}

/// MODEL

#[ffi_type(patterns(ffi_error))]
#[repr(C)]
#[derive(Debug)]
pub enum ModelError {
    Ok = 0,
    Null = 1,
    Panic = 2,
    BadModelPath = 3,
    LoadFailed = 5,
}

impl FFIError for ModelError {
    const SUCCESS: Self = Self::Ok;
    const NULL: Self = Self::Null;
    const PANIC: Self = Self::Panic;
}

#[ffi_type(opaque)]
pub struct ModelWrapper {
    model_path: std::ffi::CString,
    use_gpu: bool,
    model: Option<nobodywho::llm::Model>,
}

#[ffi_service(error = "ModelError", prefix = "modelwrapper_")]
impl ModelWrapper {
    #[ffi_service_ctor]
    pub fn new(model_path_ptr: AsciiPointer, use_gpu: bool) -> Result<Self, ModelError> {
        let Some(model_path) = model_path_ptr.as_c_str().map(|s| s.to_owned()).to_owned() else {
            error!("Model path was null pointer.");
            return Err(ModelError::BadModelPath);
        };
        Ok(Self {
            model_path,
            use_gpu,
            model: None,
        })
    }

    #[ffi_service_method(on_panic = "undefined_behavior")]
    pub fn get_use_gpu_if_available(&self) -> bool {
        self.use_gpu
    }

    pub fn set_use_gpu_if_available(&mut self, value: bool) -> Result<(), ModelError> {
        self.use_gpu = value;
        Ok(())
    }

    #[ffi_service_method(on_panic = "undefined_behavior")]
    pub fn get_model_path(&self) -> *const std::ffi::c_char {
        // important that the C# caller side copies the returned data before calling set_model_path
        // e.g. using Marshal.PtrToStringAnsi
        self.model_path.as_ptr()
    }

    pub fn set_model_path(&mut self, model_path_ptr: AsciiPointer) -> Result<(), ModelError> {
        let Some(model_path) = model_path_ptr.as_c_str().map(|s| s.to_owned()).to_owned() else {
            error!("Model path was null pointer.");
            return Err(ModelError::Null);
        };
        self.model_path = model_path;
        Ok(())
    }

    fn get_model(&mut self) -> Result<nobodywho::llm::Model, nobodywho::errors::LoadModelError> {
        if let Some(ref model) = self.model {
            return Ok(model.clone());
        }

        let model_path_str = match self.model_path.to_str() {
            Ok(s) => s,
            Err(e) => {
                error!("Model path contained invalid UTF-8.");
                return Err(nobodywho::errors::LoadModelError::InvalidModel(format!(
                    "{e}"
                )));
            }
        };

        match nobodywho::llm::get_model(model_path_str, self.use_gpu) {
            Ok(model) => {
                self.model = Some(model.clone());
                Ok(model)
            }
            Err(err) => {
                warn!("Failed loading model: {err:?}");
                Err(err)
            }
        }
    }
}

callback!(ToolCallback(input: *const std::ffi::c_void) -> *const std::ffi::c_void);

/// CHAT WORKER
#[ffi_type(patterns(ffi_error))]
#[repr(C)]
#[derive(Debug)]
pub enum ChatError {
    Ok = 0,
    Null = 1,
    Panic = 2,
    GenerationInProgress = 3,
    BadSystemPrompt = 4,
    BadSayText = 5,
    LoadModelFailed = 6,
    WorkerNotStarted = 7,
    BadName = 8,
    BadDescription = 9,
    BadJsonSchema = 10,
    BadReturnValue = 11,
}

impl FFIError for ChatError {
    const SUCCESS: Self = Self::Ok;
    const NULL: Self = Self::Null;
    const PANIC: Self = Self::Panic;
}

#[ffi_type(opaque)]
pub struct ChatWrapper {
    handle: Option<nobodywho::chat::ChatHandle>,
    response_rx: Option<tokio::sync::mpsc::Receiver<nobodywho::llm::WriteOutput>>,
    last_returned_cstring: std::ffi::CString,
    _cstring_allocation: std::ffi::CString,
    tools: Vec<nobodywho::chat::Tool>,
}

#[ffi_service(error = "ChatError", prefix = "chatwrapper_")]
impl ChatWrapper {
    #[ffi_service_ctor]
    pub fn new() -> Result<Self, ChatError> {
        Ok(ChatWrapper {
            handle: None,
            response_rx: None,
            last_returned_cstring: std::ffi::CString::default(),
            _cstring_allocation: std::ffi::CString::default(),
            tools: vec![],
        })
    }

    pub fn start_worker(
        &mut self,
        modelwrapper: &mut ModelWrapper,
        n_ctx: u32,
        system_prompt: AsciiPointer,
    ) -> Result<(), ChatError> {
        let model = modelwrapper
            .get_model()
            .map_err(|_| ChatError::LoadModelFailed)?;

        let system_prompt = system_prompt
            .as_str()
            .map_err(|_| ChatError::BadSystemPrompt)?
            .into();

        let handle = nobodywho::chat::ChatHandle::new(
            model,
            nobodywho::chat::ChatConfig {
                n_ctx,
                system_prompt,
                tools: self.tools.clone(),
                allow_thinking: true,
                sampler_config: SamplerConfig::default(),
            },
        );
        self.handle = Some(handle);
        Ok(())
    }

    pub fn reset_context(&self, system_prompt: AsciiPointer) -> Result<(), ChatError> {
        let system_prompt = system_prompt
            .as_str()
            .map_err(|_| ChatError::BadSystemPrompt)?
            .into();
        if let Some(ref handle) = self.handle {
            handle.reset_chat(system_prompt, self.tools.clone());
            Ok(())
        } else {
            Err(ChatError::WorkerNotStarted)
        }
    }

    pub fn say(
        &mut self,
        text: AsciiPointer,
        use_grammar: bool,
        grammar: AsciiPointer,
    ) -> Result<(), ChatError> {
        if self.response_rx.is_some() {
            error!("There is already a generation in progress. Please wait for it to finish before starting a new one.");
            return Err(ChatError::GenerationInProgress);
        }

        if let Some(ref mut handle) = self.handle {
            let grammar = if use_grammar {
                grammar.as_str().ok()
            } else {
                None
            };

            let sampler = grammar.map_or(SamplerConfig::default(), |g| {
                SamplerPresets::grammar(g.to_string())
            });

            handle.set_sampler_config(sampler);

            let response_rx = handle.say(text.as_str().map_err(|_| ChatError::BadSayText)?.into());

            debug_assert!(self.response_rx.is_none());
            self.response_rx = Some(response_rx);

            Ok(())
        } else {
            warn!("Worker not started yet. Please call StartWorker first.");
            Err(ChatError::WorkerNotStarted)
        }
    }

    pub fn add_tool(
        &mut self,
        callback: ToolCallback,
        name: AsciiPointer,
        description: AsciiPointer,
        json_schema: AsciiPointer,
    ) -> Result<(), ChatError> {
        if let Some(ref mut _handle) = self.handle {
            let name = name.as_str().map_err(|_| ChatError::BadName)?;
            let description = description
                .as_str()
                .map_err(|_| ChatError::BadDescription)?;
            let json_schema: serde_json::Value =
                serde_json::from_str(json_schema.as_str().map_err(|_| ChatError::BadJsonSchema)?)
                    .map_err(|_| ChatError::BadJsonSchema)?;

            let callback = move |json: serde_json::Value| -> String {
                let json_str = std::ffi::CString::new(json.to_string()).unwrap();
                let res: *const std::ffi::c_void =
                    callback.call(json_str.as_ptr() as *const std::ffi::c_void);
                // Cast back to str
                let res_str = unsafe { std::ffi::CStr::from_ptr(res as *const c_char) };
                res_str.to_str().unwrap().to_string()
            };
            let tool = nobodywho::chat::Tool::new(
                name.to_string(),
                description.to_string(),
                json_schema,
                Arc::new(callback),
            );
            self.tools.push(tool);
            Ok(())
        } else {
            Err(ChatError::WorkerNotStarted)
        }
    }

    pub fn clear_tools(&mut self) -> Result<(), ChatError> {
        self.tools.clear();
        Ok(())
    }

    #[ffi_service_method(on_panic = "return_default")]
    pub fn get_chat_history(&mut self) -> JsonPointer {
        let Some(ref handle) = self.handle else {
            return JsonPointer::default();
        };
        let mut rx = handle.get_chat_history();
        let chat_history = rx.blocking_recv();
        let json: String = serde_json::to_string(&chat_history).unwrap_or_default();
        debug!("chat_history: {json}");
        let cstring = std::ffi::CString::new(json).unwrap_or_default();
        self._cstring_allocation = cstring;
        JsonPointer {
            ptr: self._cstring_allocation.as_ptr(),
            len: self._cstring_allocation.as_bytes().len() as u32,
        }
    }

    pub fn set_chat_history(&mut self, chat_history: AsciiPointer) -> Result<(), ChatError> {
        if let Some(ref handle) = self.handle {
            let string = chat_history
                .as_str()
                .map_err(|_| ChatError::BadJsonSchema)?;
            let json: serde_json::Value =
                serde_json::from_str(string).map_err(|_| ChatError::BadJsonSchema)?;
            let messages: Vec<nobodywho::chat_state::Message> =
                serde_json::from_value(json["messages"].clone())
                    .map_err(|_| ChatError::BadJsonSchema)?;

            handle.set_chat_history(messages);
            Ok(())
        } else {
            Err(ChatError::WorkerNotStarted)
        }
    }

    pub fn stop(&mut self) -> Result<(), ChatError> {
        if let Some(ref mut handle) = self.handle {
            handle.stop_generation();
            Ok(())
        } else {
            Err(ChatError::WorkerNotStarted)
        }
    }

    pub fn set_sampler_config(
        &mut self,
        use_grammar: bool,
        grammar: AsciiPointer,
    ) -> Result<(), ChatError> {
        if let Some(ref handle) = self.handle {
            let grammar = if use_grammar {
                grammar.as_str().ok()
            } else {
                None
            };

            let sampler = grammar.map_or(SamplerConfig::default(), |g| {
                SamplerPresets::grammar(g.to_string())
            });

            handle.set_sampler_config(sampler);
            Ok(())
        } else {
            Err(ChatError::WorkerNotStarted)
        }
    }

    #[ffi_service_method(on_panic = "return_default")]
    pub fn poll_response(&mut self) -> PollResponseResult {
        use tokio::sync::mpsc::error::TryRecvError;
        if let Some(ref mut rx) = self.response_rx {
            match rx.try_recv() {
                Err(TryRecvError::Empty) => PollResponseResult::default(),
                Err(TryRecvError::Disconnected) => {
                    warn!("Could not poll. No active generation");
                    self.response_rx = None;
                    PollResponseResult::default()
                }
                Ok(nobodywho::llm::WriteOutput::Token(tok)) => {
                    debug!("Got token");
                    // store last returned cstring, so we dont have to transfer ownership
                    // on the C# side, we just need to make a copy before calling poll next time
                    // otherwise, we get UB.
                    let Ok(cstring_to_return) = std::ffi::CString::new(tok) else {
                        error!("Latest token contains a null byte.");
                        return PollResponseResult::default();
                    };
                    self.last_returned_cstring = cstring_to_return;

                    PollResponseResult {
                        kind: PollResponseKind::Token,
                        ptr: self.last_returned_cstring.as_ptr(),
                        len: self.last_returned_cstring.as_bytes().len() as u32,
                    }
                }
                Ok(nobodywho::llm::WriteOutput::Done(resp)) => {
                    debug!("Got full resp: {resp:?}");
                    // same as above
                    let Ok(cstring_to_return) = std::ffi::CString::new(resp) else {
                        error!("Latest response contains a null byte.");
                        return PollResponseResult::default();
                    };
                    self.last_returned_cstring = cstring_to_return;

                    self.response_rx = None;

                    PollResponseResult {
                        kind: PollResponseKind::Done,
                        ptr: self.last_returned_cstring.as_ptr(),
                        len: self.last_returned_cstring.as_bytes().len() as u32,
                    }
                }
            }
        } else {
            PollResponseResult::default()
        }
    }
}

#[ffi_type(patterns(ffi_enum))]
#[repr(C)]
pub enum PollResponseKind {
    Nothing = 0,
    Token = 1,
    Done = 2,
}

#[ffi_type]
#[repr(C)]
pub struct PollResponseResult {
    pub kind: PollResponseKind,
    pub ptr: *const std::ffi::c_char,
    pub len: u32,
}

impl Default for PollResponseResult {
    fn default() -> Self {
        Self {
            kind: PollResponseKind::Nothing,
            ptr: std::ptr::null(),
            len: 0,
        }
    }
}

#[ffi_type]
#[repr(C)]
pub struct JsonPointer {
    pub ptr: *const std::ffi::c_char,
    pub len: u32,
}

impl Default for JsonPointer {
    fn default() -> Self {
        Self {
            ptr: std::ptr::null(),
            len: 0,
        }
    }
}
/// ENCODER

#[ffi_type(patterns(ffi_error))]
#[repr(C)]
#[derive(Debug)]
pub enum EncodeError {
    Ok = 0,
    Null = 1,
    Panic = 2,
    GenerationInProgress = 3,
    BadEncodeText = 4,
    LoadModelFailed = 5,
    WorkerNotStarted = 6,
}

impl FFIError for EncodeError {
    const SUCCESS: Self = Self::Ok;
    const NULL: Self = Self::Null;
    const PANIC: Self = Self::Panic;
}

#[ffi_type(opaque)]
pub struct EncoderWrapper {
    handle: Option<nobodywho::encoder::EncoderAsync>,
    response_rx: Option<tokio::sync::mpsc::Receiver<Vec<f32>>>,
    last_returned_encoding: Vec<f32>,
}

#[ffi_service(error = "EncodeError", prefix = "encoderwrapper_")]
impl EncoderWrapper {
    #[ffi_service_ctor]
    pub fn new() -> Result<Self, EncodeError> {
        Ok(EncoderWrapper {
            handle: None,
            response_rx: None,
            last_returned_encoding: vec![],
        })
    }

    pub fn start_worker(
        &mut self,
        modelwrapper: &mut ModelWrapper,
        n_ctx: u32,
    ) -> Result<(), EncodeError> {
        let model = modelwrapper
            .get_model()
            .map_err(|_| EncodeError::LoadModelFailed)?;
        let handle = nobodywho::encoder::EncoderAsync::new(model, n_ctx);
        self.handle = Some(handle);
        Ok(())
    }

    pub fn encode(&mut self, text: AsciiPointer) -> Result<(), EncodeError> {
        if self.response_rx.is_some() {
            error!("There is already a generation in progress. Please wait for it to finish before starting a new one.");
            return Err(EncodeError::GenerationInProgress);
        }

        let text = text
            .as_str()
            .map_err(|_| EncodeError::BadEncodeText)?
            .to_string();
        if let Some(ref mut handle) = self.handle {
            let response_rx = handle.encode(text);
            debug_assert!(self.response_rx.is_none());
            self.response_rx = Some(response_rx);
            Ok(())
        } else {
            Err(EncodeError::WorkerNotStarted)
        }
    }

    #[ffi_service_method(on_panic = "undefined_behavior")]
    pub fn poll_encoding(&mut self) -> FFISlice<'_, f32> {
        use tokio::sync::mpsc::error::TryRecvError;
        if let Some(ref mut rx) = self.response_rx {
            match rx.try_recv() {
                Err(TryRecvError::Empty) => FFISlice::default(),
                Err(TryRecvError::Disconnected) => {
                    warn!("Could not poll. No active generation");
                    self.response_rx = None;
                    FFISlice::default()
                }
                Ok(encoding) => {
                    self.last_returned_encoding = encoding;
                    self.response_rx = None;
                    return FFISlice::from_slice(self.last_returned_encoding.as_slice());
                }
            }
        } else {
            FFISlice::<f32>::default()
        }
    }
}

#[ffi_function]
#[no_mangle]
pub extern "C" fn cosine_similarity(a: FFISlice<f32>, b: FFISlice<f32>) -> f32 {
    return nobodywho::encoder::cosine_similarity(a.as_slice(), b.as_slice());
}

/// BINDINGS

pub fn my_inventory() -> Inventory {
    InventoryBuilder::new()
        .register(function!(init_tracing))
        .register(pattern!(ModelWrapper))
        .register(pattern!(ChatWrapper))
        .register(pattern!(EncoderWrapper))
        .register(function!(cosine_similarity))
        .inventory()
}

#[test]
fn bindings_csharp() -> Result<(), interoptopus::Error> {
    // this is just for (ab)using `cargo test` to generate bindings
    use interoptopus::util::NamespaceMappings;
    use interoptopus::Interop;
    use interoptopus_backend_csharp::{Config, Generator, ParamSliceType};

    let config = Config {
        dll_name: "nobodywho_unity".to_string(),
        class: "NobodyWhoBindings".into(),
        namespace_mappings: NamespaceMappings::new("NobodyWho"),
        param_slice_type: ParamSliceType::Array,
        ..Config::default()
    };

    // Generate the bindings
    Generator::new(config, my_inventory()).write_file("./src/Runtime/NobodyWhoBindings.cs")?;

    // This is kind of ugly but i dont see a better way (unless we overwrite the config).
    // Post-process the generated file to add version logging
    let mut content = std::fs::read_to_string("./src/Runtime/NobodyWhoBindings.cs")?;
    content = content.replace(
        "static NobodyWhoBindings()\n        {\n        }",
        &format!(
        "static NobodyWhoBindings()\n        {{\n            UnityEngine.Debug.Log(\"NobodyWho Library Version: {}\");\n        }}",
            env!("CARGO_PKG_VERSION")
        )
    );
    std::fs::write("./src/Runtime/NobodyWhoBindings.cs", content)?;

    Ok(())
}
