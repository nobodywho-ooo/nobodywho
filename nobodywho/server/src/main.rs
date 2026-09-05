use axum::extract::rejection::JsonRejection;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use nobodywho::chat::{
    ChatBuilder, ChatHandleAsync, CompletionResponse, Message, Options, StructuredCompletionChunk,
    StructuredCompletionStreamAsync, TurnOptions,
};
use nobodywho::llm;
use nobodywho::tool_calling::{Tool, ToolCall};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;
use tracing::info;
use tracing_subscriber::EnvFilter;

const DEFAULT_HOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const DEFAULT_PORT: u16 = 8888;
const DEFAULT_CONTEXT_SIZE: u32 = 16384;
const ROOT_PATH: &str = "/";
const HEALTH_PATH: &str = "/health";
const MODELS_PATH: &str = "/v1/models";
const CHAT_COMPLETIONS_PATH: &str = "/v1/chat/completions";
const ROUTES: [(&str, &str); 4] = [
    ("GET", ROOT_PATH),
    ("GET", HEALTH_PATH),
    ("GET", MODELS_PATH),
    ("POST", CHAT_COMPLETIONS_PATH),
];
const THINK_START: &str = "<think>";
const THINK_END: &str = "</think>";
const CONTENT_MARKERS: [&str; 1] = [THINK_START];
const THINKING_MARKERS: [&str; 1] = [THINK_END];
const CONTENT_MARKERS_WITH_TOOLS: [&str; 6] = [
    THINK_START,
    "<tool_call>",
    "<start_function_call>",
    "<|tool_call>",
    "[TOOL_CALLS]",
    "<|tool_call_start|>",
];

#[derive(Parser)]
#[command(about = "Serve a NobodyWho model through the OpenAI Chat Completions API")]
struct ServerConfig {
    /// Local GGUF path, HTTP URL, Hugging Face reference, or auto.
    #[arg(long)]
    model: String,

    #[arg(long)]
    name: Option<String>,

    #[arg(long, default_value_t = DEFAULT_HOST)]
    host: IpAddr,

    #[arg(long, default_value_t = DEFAULT_PORT)]
    port: u16,

    #[arg(long, default_value_t = DEFAULT_CONTEXT_SIZE)]
    context_size: u32,

    #[arg(long)]
    threads: Option<u32>,

    /// Disable GPU acceleration.
    #[arg(long)]
    cpu: bool,
}

#[derive(Clone)]
struct AppState {
    chat: ChatHandleAsync,
    model: ModelInfo,
    request_lock: Arc<Mutex<()>>,
    request_ids: Arc<AtomicU64>,
}

#[derive(Clone)]
struct ModelInfo {
    id: String,
    context_window: u32,
    max_tokens: usize,
}

#[derive(Deserialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<RequestMessage>,
    #[serde(default, deserialize_with = "deserialize_tools")]
    tools: Vec<Tool>,
    tool_choice: Option<ToolChoice>,
    #[serde(default)]
    stream: bool,
    max_tokens: Option<usize>,
    max_completion_tokens: Option<usize>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    seed: Option<u32>,
}

impl ChatCompletionRequest {
    fn output_limit(&self) -> Result<Option<usize>, String> {
        if self.max_tokens.is_some() && self.max_completion_tokens.is_some() {
            return Err("pass max_tokens or max_completion_tokens, not both".to_string());
        }
        Ok(self.max_completion_tokens.or(self.max_tokens))
    }
}

#[derive(Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
enum RequestMessage {
    System {
        content: RequestContent,
    },
    Developer {
        content: RequestContent,
    },
    User {
        content: RequestContent,
    },
    Assistant {
        #[serde(default)]
        content: Option<RequestContent>,
        #[serde(default)]
        tool_calls: Vec<RequestToolCall>,
    },
    Tool {
        content: RequestContent,
        tool_call_id: String,
        name: Option<String>,
    },
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RequestContent {
    Text(String),
    Parts(Vec<RequestContentPart>),
}

impl RequestContent {
    fn into_text(self) -> Result<String, String> {
        match self {
            Self::Text(text) => Ok(text),
            Self::Parts(parts) => parts
                .into_iter()
                .map(|part| match part.kind.as_str() {
                    "text" => part
                        .text
                        .ok_or_else(|| "text content is missing text".to_string()),
                    _ => Err(format!("unsupported content type: {}", part.kind)),
                })
                .collect::<Result<Vec<_>, _>>()
                .map(|parts| parts.join("")),
        }
    }
}

#[derive(Deserialize)]
struct RequestContentPart {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct RequestToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    function: RequestFunctionCall,
}

#[derive(Deserialize)]
struct RequestFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum FunctionType {
    Function,
}

#[derive(Deserialize)]
struct ToolDefinition {
    #[serde(rename = "type")]
    _kind: FunctionType,
    function: FunctionDefinition,
}

#[derive(Deserialize)]
struct FunctionDefinition {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    parameters: serde_json::Map<String, Value>,
}

fn deserialize_tools<'de, D>(deserializer: D) -> Result<Vec<Tool>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Vec::<ToolDefinition>::deserialize(deserializer)?
        .into_iter()
        .map(|definition| {
            Tool::new(
                definition.function.name,
                definition.function.description,
                Value::Object(definition.function.parameters),
                Arc::new(|_| String::new()),
            )
        })
        .collect())
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum ToolChoiceMode {
    Auto,
    None,
    Required,
}

#[derive(Deserialize)]
struct ToolChoiceFunction {
    name: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ToolChoice {
    Mode(ToolChoiceMode),
    Function {
        #[serde(rename = "type")]
        _kind: FunctionType,
        function: ToolChoiceFunction,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_env_filter(EnvFilter::new("info,llama-cpp-2=off"))
        .init();
    nobodywho::send_llamacpp_logs_to_tracing();
    info!("nobodywho-server is experimental and subject to change");
    let config = ServerConfig::parse();

    let model =
        Arc::new(llm::get_model_async(config.model.clone(), !config.cpu, None, None, None).await?);
    let context_window = config.context_size.min(model.max_ctx());
    let mut builder = ChatBuilder::new(model).with_context_size(context_window);
    if let Some(threads) = config.threads {
        builder = builder.with_n_threads(threads);
    }
    let chat = builder.build_async()?;
    let model_id = config.name.unwrap_or_else(|| model_id_for(&config.model));

    let state = AppState {
        chat,
        model: ModelInfo {
            id: model_id,
            context_window,
            max_tokens: context_window as usize,
        },
        request_lock: Arc::new(Mutex::new(())),
        request_ids: Arc::new(AtomicU64::new(1)),
    };
    let listener = tokio::net::TcpListener::bind((config.host, config.port)).await?;
    let summary = StartupSummary {
        address: listener.local_addr()?,
        model: &state.model,
    };
    info!("{summary}");
    let app = Router::new()
        .route(ROOT_PATH, get(health))
        .route(HEALTH_PATH, get(health))
        .route(MODELS_PATH, get(models))
        .route(CHAT_COMPLETIONS_PATH, post(chat_completions))
        .with_state(state);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

#[cfg(unix)]
async fn shutdown_signal() {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("failed to install SIGTERM handler");
    tokio::select! {
        result = tokio::signal::ctrl_c() => result.expect("failed to install Ctrl-C handler"),
        _ = terminate.recv() => {},
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install Ctrl-C handler");
}

async fn health() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

async fn models(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "object": "list",
        "data": [{
            "id": state.model.id,
            "object": "model",
            "created": 0,
            "owned_by": "nobodywho",
            "name": state.model.id,
            "context_window": state.model.context_window,
            "max_tokens": state.model.max_tokens,
        }]
    }))
}

async fn chat_completions(
    State(state): State<AppState>,
    request: Result<Json<ChatCompletionRequest>, JsonRejection>,
) -> Response {
    let Json(request) = match request {
        Ok(request) => request,
        Err(error) => return api_error(error.status(), error.body_text()),
    };
    match start_completion(&state, request).await {
        Ok(response) => response,
        Err((status, message)) => api_error(status, message),
    }
}

async fn start_completion(
    state: &AppState,
    request: ChatCompletionRequest,
) -> Result<Response, (StatusCode, String)> {
    let output_limit = request.output_limit().map_err(bad_request)?;
    let ChatCompletionRequest {
        model,
        messages,
        tools,
        tool_choice,
        stream: should_stream,
        temperature,
        top_p,
        seed,
        ..
    } = request;
    if model != state.model.id {
        return Err((StatusCode::NOT_FOUND, format!("model not found: {model}")));
    }
    let messages = convert_messages(messages)?;
    let tools = select_tools(tools, tool_choice)?;
    let has_tools = !tools.is_empty();
    let max_tokens = output_limit.unwrap_or(state.model.max_tokens);
    if max_tokens == 0 || max_tokens > state.model.max_tokens {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "max_tokens must be between 1 and {}",
                state.model.max_tokens
            ),
        ));
    }

    let options = Options::new().with_tools(tools);
    let turn_options = TurnOptions::new()
        .with_sampling_overrides(temperature, top_p, seed)
        .map_err(|error| bad_request(error.to_string()))?
        .with_max_tokens(max_tokens);
    let stream = {
        let _request_guard = state.request_lock.lock().await;
        state
            .chat
            .set_system_prompt(None)
            .await
            .map_err(internal_error)?;
        state
            .chat
            .complete_with_external_tools_and_metadata(messages, options, turn_options)
            .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?
    };

    let sequence = state.request_ids.fetch_add(1, Ordering::Relaxed);
    let created = unix_timestamp();
    let id = format!("chatcmpl-{created}-{sequence}");
    let response_model = model;

    if should_stream {
        return Ok(streaming_response(
            stream,
            id,
            response_model,
            created,
            has_tools,
        ));
    }

    let response = buffered_response(stream, id, response_model, created).await?;
    Ok(Json(response).into_response())
}

fn convert_messages(messages: Vec<RequestMessage>) -> Result<Vec<Message>, (StatusCode, String)> {
    let mut converted = Vec::with_capacity(messages.len());
    let mut system_parts = Vec::new();
    let mut tool_names = HashMap::new();
    let mut saw_conversation = false;

    for message in messages {
        match message {
            RequestMessage::System { content } | RequestMessage::Developer { content }
                if !saw_conversation =>
            {
                system_parts.push(content.into_text().map_err(bad_request)?);
            }
            RequestMessage::System { .. } | RequestMessage::Developer { .. } => {
                return Err(bad_request("system and developer messages must come first"));
            }
            RequestMessage::User { content } => {
                push_system_message(&mut converted, &mut system_parts);
                saw_conversation = true;
                converted.push(Message::new_user(content.into_text().map_err(bad_request)?));
            }
            RequestMessage::Assistant {
                content,
                tool_calls,
            } => {
                push_system_message(&mut converted, &mut system_parts);
                saw_conversation = true;
                let calls = tool_calls
                    .into_iter()
                    .map(|call| {
                        if call.kind != "function" {
                            return Err(format!("unsupported tool call type: {}", call.kind));
                        }
                        let arguments = if call.function.arguments.is_empty() {
                            json!({})
                        } else {
                            serde_json::from_str(&call.function.arguments).map_err(|error| {
                                format!(
                                    "invalid arguments for tool {}: {error}",
                                    call.function.name
                                )
                            })?
                        };
                        if tool_names
                            .insert(call.id.clone(), call.function.name.clone())
                            .is_some()
                        {
                            return Err(format!("duplicate tool call ID: {}", call.id));
                        }
                        Ok(ToolCall {
                            name: call.function.name,
                            arguments,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()
                    .map_err(bad_request)?;
                converted.push(Message::Assistant {
                    content: content
                        .map(RequestContent::into_text)
                        .transpose()
                        .map_err(bad_request)?
                        .unwrap_or_default()
                        .into(),
                    tool_calls: (!calls.is_empty()).then_some(calls),
                });
            }
            RequestMessage::Tool {
                content,
                tool_call_id,
                name,
            } => {
                push_system_message(&mut converted, &mut system_parts);
                saw_conversation = true;
                let expected_name = tool_names.get(&tool_call_id).ok_or_else(|| {
                    bad_request(format!(
                        "tool message references unknown tool call: {tool_call_id}"
                    ))
                })?;
                if name.as_ref().is_some_and(|name| name != expected_name) {
                    return Err(bad_request(format!(
                        "tool message name does not match tool call {tool_call_id}"
                    )));
                }
                converted.push(Message::new_tool(
                    expected_name.clone(),
                    content.into_text().map_err(bad_request)?,
                ));
            }
        }
    }

    push_system_message(&mut converted, &mut system_parts);
    Ok(converted)
}

fn push_system_message(converted: &mut Vec<Message>, system_parts: &mut Vec<String>) {
    if !system_parts.is_empty() {
        converted.push(Message::new_system(
            std::mem::take(system_parts).join("\n\n"),
        ));
    }
}

fn select_tools(
    tools: Vec<Tool>,
    tool_choice: Option<ToolChoice>,
) -> Result<Vec<Tool>, (StatusCode, String)> {
    match tool_choice {
        Some(ToolChoice::Mode(ToolChoiceMode::None)) => Ok(Vec::new()),
        Some(ToolChoice::Mode(ToolChoiceMode::Required)) => {
            Err(bad_request("tool_choice required is not supported"))
        }
        Some(ToolChoice::Function { function, .. }) => {
            let name = function.name;
            let tool = tools
                .into_iter()
                .find(|tool| tool.name == name)
                .ok_or_else(|| bad_request(format!("tool_choice names unknown tool: {name}")))?;
            Ok(vec![tool])
        }
        Some(ToolChoice::Mode(ToolChoiceMode::Auto)) | None => Ok(tools),
    }
}

async fn buffered_response(
    mut stream: StructuredCompletionStreamAsync,
    id: String,
    model: String,
    created: u64,
) -> Result<Value, (StatusCode, String)> {
    while let Some(chunk) = stream.next().await.map_err(internal_error)? {
        match chunk {
            StructuredCompletionChunk::Token(_) => {}
            StructuredCompletionChunk::Done(response) => {
                let finish_reason = response.finish_reason.as_str();
                let usage = json!({
                    "prompt_tokens": response.usage.prompt_tokens,
                    "completion_tokens": response.usage.completion_tokens,
                    "total_tokens": response.usage.total_tokens(),
                });
                let message = response_message(response, &id);
                return Ok(json!({
                    "id": id,
                    "object": "chat.completion",
                    "created": created,
                    "model": model,
                    "choices": [{
                        "index": 0,
                        "message": message,
                        "finish_reason": finish_reason,
                    }],
                    "usage": usage,
                }));
            }
        }
    }
    Err(internal_error("completion stream ended without a response"))
}

#[derive(Clone, Copy, PartialEq)]
enum StreamSection {
    Content,
    Thinking,
    ToolCall,
}

enum ParsedDelta {
    Content(String),
    Thinking(String),
}

struct ResponseStreamParser {
    section: StreamSection,
    pending: String,
    tool_text: String,
    has_tools: bool,
}

impl ResponseStreamParser {
    fn new(has_tools: bool) -> Self {
        Self {
            section: StreamSection::Content,
            pending: String::new(),
            tool_text: String::new(),
            has_tools,
        }
    }

    fn push(&mut self, token: &str) -> Vec<ParsedDelta> {
        self.pending.push_str(token);
        self.drain(false)
    }

    fn finish(mut self, has_tool_calls: bool) -> Vec<ParsedDelta> {
        let mut deltas = self.drain(true);
        if self.section == StreamSection::ToolCall && !has_tool_calls {
            self.tool_text.push_str(&self.pending);
            deltas.push(ParsedDelta::Content(self.tool_text));
        }
        deltas
    }

    fn drain(&mut self, finished: bool) -> Vec<ParsedDelta> {
        let mut deltas = Vec::new();
        loop {
            if self.section == StreamSection::ToolCall {
                self.tool_text.push_str(&self.pending);
                self.pending.clear();
                break;
            }

            if let Some((position, marker)) = self.next_marker() {
                self.emit_prefix(position, &mut deltas);
                self.pending.drain(..marker.len());
                if marker == THINK_START {
                    self.section = StreamSection::Thinking;
                } else if marker == THINK_END {
                    self.section = StreamSection::Content;
                } else {
                    self.section = StreamSection::ToolCall;
                    self.tool_text.push_str(marker);
                }
                continue;
            }

            let held = if finished {
                0
            } else {
                self.marker_prefix_len()
            };
            let ready = self.pending.len() - held;
            self.emit_prefix(ready, &mut deltas);
            break;
        }
        deltas
    }

    fn next_marker(&self) -> Option<(usize, &'static str)> {
        self.markers()
            .iter()
            .copied()
            .filter_map(|marker| self.pending.find(marker).map(|position| (position, marker)))
            .min_by_key(|(position, _)| *position)
    }

    fn marker_prefix_len(&self) -> usize {
        self.markers()
            .iter()
            .copied()
            .map(|marker| {
                (1..marker.len())
                    .rev()
                    .find(|length| self.pending.ends_with(&marker[..*length]))
                    .unwrap_or(0)
            })
            .max()
            .unwrap_or(0)
    }

    fn markers(&self) -> &'static [&'static str] {
        match (self.section, self.has_tools) {
            (StreamSection::Content, true) => &CONTENT_MARKERS_WITH_TOOLS,
            (StreamSection::Content, false) => &CONTENT_MARKERS,
            (StreamSection::Thinking, _) => &THINKING_MARKERS,
            (StreamSection::ToolCall, _) => &[],
        }
    }

    fn emit_prefix(&mut self, length: usize, deltas: &mut Vec<ParsedDelta>) {
        if length == 0 {
            return;
        }
        let text = self.pending[..length].to_string();
        self.pending.drain(..length);
        deltas.push(match self.section {
            StreamSection::Content => ParsedDelta::Content(text),
            StreamSection::Thinking => ParsedDelta::Thinking(text),
            StreamSection::ToolCall => unreachable!(),
        });
    }
}

fn streaming_response(
    mut stream: StructuredCompletionStreamAsync,
    id: String,
    model: String,
    created: u64,
    has_tools: bool,
) -> Response {
    let (sender, receiver) = mpsc::channel(32);
    tokio::spawn(async move {
        if send_event(
            &sender,
            completion_chunk(&id, &model, created, json!({"role": "assistant"}), None),
        )
        .await
        .is_err()
        {
            return;
        }

        let mut parser = ResponseStreamParser::new(has_tools);
        loop {
            match stream.next().await {
                Ok(Some(StructuredCompletionChunk::Token(token))) => {
                    for delta in parser.push(&token) {
                        let chunk =
                            completion_chunk(&id, &model, created, parsed_delta_value(delta), None);
                        if send_event(&sender, chunk).await.is_err() {
                            return;
                        }
                    }
                }
                Ok(Some(StructuredCompletionChunk::Done(response))) => {
                    for delta in parser.finish(!response.tool_calls.is_empty()) {
                        let chunk =
                            completion_chunk(&id, &model, created, parsed_delta_value(delta), None);
                        if send_event(&sender, chunk).await.is_err() {
                            return;
                        }
                    }
                    if !response.tool_calls.is_empty() {
                        let tool_calls = response_tool_calls(&response.tool_calls, &id);
                        let chunk = completion_chunk(
                            &id,
                            &model,
                            created,
                            json!({"tool_calls": tool_calls}),
                            None,
                        );
                        if send_event(&sender, chunk).await.is_err() {
                            return;
                        }
                    }
                    let reason = response.finish_reason.as_str();
                    let final_chunk =
                        completion_chunk(&id, &model, created, json!({}), Some(reason));
                    if send_event(&sender, final_chunk).await.is_err() {
                        return;
                    }
                    let _ = sender.send(Ok(Event::default().data("[DONE]"))).await;
                    return;
                }
                Ok(None) => return,
                Err(error) => {
                    let _ = send_event(&sender, error_body(error.to_string())).await;
                    let _ = sender.send(Ok(Event::default().data("[DONE]"))).await;
                    return;
                }
            }
        }
    });

    Sse::new(ReceiverStream::new(receiver))
        .keep_alive(KeepAlive::default())
        .into_response()
}

fn parsed_delta_value(delta: ParsedDelta) -> Value {
    match delta {
        ParsedDelta::Content(content) => json!({"content": content}),
        ParsedDelta::Thinking(thinking) => json!({"reasoning_content": thinking}),
    }
}

async fn send_event(
    sender: &mpsc::Sender<Result<Event, Infallible>>,
    value: Value,
) -> Result<(), ()> {
    sender
        .send(Ok(Event::default().data(value.to_string())))
        .await
        .map_err(|_| ())
}

fn completion_chunk(
    id: &str,
    model: &str,
    created: u64,
    delta: Value,
    finish_reason: Option<&str>,
) -> Value {
    json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": created,
        "model": model,
        "choices": [{
            "index": 0,
            "delta": delta,
            "finish_reason": finish_reason,
        }]
    })
}

fn response_message(response: CompletionResponse, id: &str) -> Value {
    let (content, reasoning) = split_response_content(&response.content);
    let mut message = if response.tool_calls.is_empty() {
        json!({"role": "assistant", "content": content})
    } else {
        json!({
            "role": "assistant",
            "content": if content.is_empty() { Value::Null } else { Value::String(content) },
            "tool_calls": response_tool_calls(&response.tool_calls, id),
        })
    };
    if !reasoning.is_empty() {
        message["reasoning_content"] = Value::String(reasoning);
    }
    message
}

fn split_response_content(response: &str) -> (String, String) {
    let mut parser = ResponseStreamParser::new(false);
    let mut deltas = parser.push(response);
    deltas.extend(parser.finish(false));
    let mut content = String::new();
    let mut reasoning = String::new();
    for delta in deltas {
        match delta {
            ParsedDelta::Content(text) => content.push_str(&text),
            ParsedDelta::Thinking(text) => reasoning.push_str(&text),
        }
    }
    (content, reasoning)
}

fn response_tool_calls(tool_calls: &[ToolCall], response_id: &str) -> Vec<Value> {
    tool_calls
        .iter()
        .enumerate()
        .map(|(index, call)| {
            json!({
                "index": index,
                "id": format!("call-{response_id}-{index}"),
                "type": "function",
                "function": {
                    "name": call.name,
                    "arguments": call.arguments.to_string(),
                }
            })
        })
        .collect()
}

fn model_id_for(source: &str) -> String {
    source
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(source)
        .strip_suffix(".gguf")
        .unwrap_or_else(|| source.rsplit('/').next().unwrap_or(source))
        .to_string()
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

struct StartupSummary<'a> {
    address: SocketAddr,
    model: &'a ModelInfo,
}

impl fmt::Display for StartupSummary<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let base_url = format!("http://{}", self.address);
        let routes = ROUTES
            .iter()
            .map(|(method, path)| format!("    {method:<4} {path}"))
            .collect::<Vec<_>>()
            .join("\n");
        let completion_request = json!({
            "model": self.model.id,
            "messages": [{"role": "user", "content": "Say hello in one short sentence."}],
            "stream": true,
        });
        writeln!(formatter, "Server ready")?;
        writeln!(formatter)?;
        writeln!(formatter, "  Address:  {base_url}")?;
        writeln!(formatter, "  Model ID: {}", self.model.id)?;
        writeln!(
            formatter,
            "  Context:  {} tokens",
            self.model.context_window
        )?;
        writeln!(formatter)?;
        writeln!(formatter, "  Routes:")?;
        writeln!(formatter, "{routes}")?;
        writeln!(formatter)?;
        writeln!(formatter, "  Try it:")?;
        writeln!(formatter, "    curl {base_url}{HEALTH_PATH}")?;
        writeln!(formatter, "    curl {base_url}{MODELS_PATH}")?;
        writeln!(
            formatter,
            "    curl -N {base_url}{CHAT_COMPLETIONS_PATH} \\\n      -H 'Content-Type: application/json' \\\n      -d '{completion_request}'"
        )
    }
}

fn bad_request(message: impl ToString) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, message.to_string())
}

fn internal_error(message: impl ToString) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, message.to_string())
}

fn error_body(message: impl ToString) -> Value {
    json!({
        "error": {
            "message": message.to_string(),
            "type": "invalid_request_error",
            "param": null,
            "code": null,
        }
    })
}

fn api_error(status: StatusCode, message: impl ToString) -> Response {
    (status, Json(error_body(message))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_pi_compatible_defaults() {
        let config =
            ServerConfig::try_parse_from(["nobodywho-server", "--model", "model.gguf"]).unwrap();

        assert_eq!(config.host, DEFAULT_HOST);
        assert_eq!(config.port, DEFAULT_PORT);
        assert_eq!(config.context_size, DEFAULT_CONTEXT_SIZE);
    }

    #[test]
    fn startup_summary_uses_configured_routes() {
        let model = ModelInfo {
            id: "qwen".into(),
            context_window: DEFAULT_CONTEXT_SIZE,
            max_tokens: DEFAULT_CONTEXT_SIZE as usize,
        };
        let summary = StartupSummary {
            address: SocketAddr::new(DEFAULT_HOST, DEFAULT_PORT),
            model: &model,
        }
        .to_string();

        for (method, path) in ROUTES {
            assert!(summary.contains(&format!("{method:<4} {path}")));
        }
        assert!(summary.contains(&format!("http://{DEFAULT_HOST}:{DEFAULT_PORT}")));
    }

    #[test]
    fn accepts_initial_developer_messages() {
        let messages = vec![
            RequestMessage::System {
                content: RequestContent::Text("system".into()),
            },
            RequestMessage::Developer {
                content: RequestContent::Text("developer".into()),
            },
            RequestMessage::User {
                content: RequestContent::Text("hello".into()),
            },
        ];

        let converted = convert_messages(messages).unwrap();
        assert!(matches!(
            converted.first(),
            Some(Message::System { content }) if content.to_string() == "system\n\ndeveloper"
        ));
    }

    #[test]
    fn maps_tool_result_ids_back_to_names() {
        let messages = vec![
            RequestMessage::User {
                content: RequestContent::Text("weather?".into()),
            },
            RequestMessage::Assistant {
                content: None,
                tool_calls: vec![RequestToolCall {
                    id: "call-1".into(),
                    kind: "function".into(),
                    function: RequestFunctionCall {
                        name: "weather".into(),
                        arguments: r#"{"city":"Copenhagen"}"#.into(),
                    },
                }],
            },
            RequestMessage::Tool {
                content: RequestContent::Text("sunny".into()),
                tool_call_id: "call-1".into(),
                name: None,
            },
        ];

        let converted = convert_messages(messages).unwrap();
        assert!(matches!(
            converted.last(),
            Some(Message::Tool { name, .. }) if name == "weather"
        ));
    }

    #[test]
    fn rejects_mismatched_tool_result_names() {
        let messages = vec![
            RequestMessage::Assistant {
                content: None,
                tool_calls: vec![RequestToolCall {
                    id: "call-1".into(),
                    kind: "function".into(),
                    function: RequestFunctionCall {
                        name: "weather".into(),
                        arguments: "{}".into(),
                    },
                }],
            },
            RequestMessage::Tool {
                content: RequestContent::Text("sunny".into()),
                tool_call_id: "call-1".into(),
                name: Some("calendar".into()),
            },
        ];

        assert!(convert_messages(messages).is_err());
    }

    #[test]
    fn parses_tool_definitions() {
        let request: ChatCompletionRequest = serde_json::from_value(json!({
            "model": "qwen",
            "messages": [],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "weather",
                    "description": "Get the weather",
                    "parameters": {"type": "object"}
                }
            }]
        }))
        .unwrap();

        assert_eq!(request.tools.len(), 1);
        assert_eq!(request.tools[0].name, "weather");
        assert_eq!(request.tools[0].json_schema, json!({"type": "object"}));
    }

    #[test]
    fn rejects_conflicting_output_limits() {
        let request: ChatCompletionRequest = serde_json::from_value(json!({
            "model": "model",
            "messages": [{"role": "user", "content": "hello"}],
            "max_tokens": 10,
            "max_completion_tokens": 20,
        }))
        .unwrap();

        assert_eq!(
            request.output_limit().unwrap_err(),
            "pass max_tokens or max_completion_tokens, not both"
        );
    }

    #[test]
    fn accepts_auto_tool_choice() {
        let choice = serde_json::from_value(json!("auto")).unwrap();
        assert!(select_tools(Vec::new(), Some(choice)).is_ok());
    }

    #[test]
    fn rejects_unsupported_required_tool_choice() {
        let choice = serde_json::from_value(json!("required")).unwrap();
        assert!(select_tools(Vec::new(), Some(choice)).is_err());
    }

    #[test]
    fn streams_thinking_separately_across_split_markers() {
        let mut parser = ResponseStreamParser::new(false);
        let mut deltas = Vec::new();
        for token in ["<thi", "nk>plan", "</thi", "nk>answer"] {
            deltas.extend(parser.push(token));
        }
        deltas.extend(parser.finish(false));
        let values: Vec<Value> = deltas.into_iter().map(parsed_delta_value).collect();

        assert_eq!(
            values,
            vec![
                json!({"reasoning_content": "plan"}),
                json!({"content": "answer"}),
            ]
        );
    }

    #[test]
    fn streams_content_before_suppressing_tool_syntax() {
        let mut parser = ResponseStreamParser::new(true);
        let mut deltas = parser.push("answer<tool_");
        deltas.extend(parser.push("call>hidden"));
        deltas.extend(parser.finish(true));
        let values: Vec<Value> = deltas.into_iter().map(parsed_delta_value).collect();

        assert_eq!(values, vec![json!({"content": "answer"})]);
    }

    #[test]
    fn restores_tool_marker_when_no_tool_call_was_parsed() {
        let mut parser = ResponseStreamParser::new(true);
        let mut deltas = parser.push("literal <tool_call> text");
        deltas.extend(parser.finish(false));
        let values: Vec<Value> = deltas.into_iter().map(parsed_delta_value).collect();

        assert_eq!(
            values,
            vec![
                json!({"content": "literal "}),
                json!({"content": "<tool_call> text"}),
            ]
        );
    }

    #[test]
    fn returns_buffered_thinking_separately() {
        let message = response_message(
            CompletionResponse {
                content: "<think>plan</think>answer".into(),
                tool_calls: Vec::new(),
                finish_reason: nobodywho::chat::FinishReason::Stop,
                usage: nobodywho::chat::CompletionUsage::default(),
            },
            "chatcmpl-1",
        );

        assert_eq!(message["reasoning_content"], "plan");
        assert_eq!(message["content"], "answer");
    }

    #[test]
    fn returns_openai_tool_calls() {
        let calls = response_tool_calls(
            &[ToolCall {
                name: "weather".into(),
                arguments: json!({"city": "Copenhagen"}),
            }],
            "chatcmpl-1",
        );
        assert_eq!(calls[0]["function"]["name"], "weather");
        assert_eq!(
            calls[0]["function"]["arguments"],
            r#"{"city":"Copenhagen"}"#
        );
    }
}
