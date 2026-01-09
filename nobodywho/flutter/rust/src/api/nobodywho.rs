use flutter_rust_bridge::{DartFnFuture, Rust2DartSendError};
// ^ in general I've only done fully-qualified imports, but these things need to be imported to
// satisfy some frb macros

#[flutter_rust_bridge::frb(opaque)]
pub struct NobodyWhoModel {
    model: nobodywho::llm::Model,
}

impl NobodyWhoModel {
    #[flutter_rust_bridge::frb(sync)]
    pub fn new(model_path: &str, #[frb(default = true)] use_gpu: bool) -> Result<Self, String> {
        let model = nobodywho::llm::get_model(model_path, use_gpu).map_err(|e| e.to_string())?;
        Ok(Self { model })
    }
}

#[flutter_rust_bridge::frb(opaque)]
pub struct NobodyWhoChat {
    chat: nobodywho::chat::ChatHandleAsync,
}

impl NobodyWhoChat {
    #[flutter_rust_bridge::frb(sync)]
    pub fn new(
        model: NobodyWhoModel,
        system_prompt: String,
        context_size: u32,
        tools: Vec<NobodyWhoTool>,
    ) -> Self {
        let chat = nobodywho::chat::ChatBuilder::new(model.model)
            .with_system_prompt(system_prompt)
            .with_context_size(context_size)
            .with_tools(tools.into_iter().map(|t| t.tool).collect())
            .build_async();
        Self { chat }
    }

    #[flutter_rust_bridge::frb(sync)]
    pub fn ask(&self, message: String) -> NobodyWhoTokenStream {
        NobodyWhoTokenStream {
            stream: self.chat.ask(message),
        }
    }

    pub async fn get_chat_history(
        &self,
    ) -> Result<Vec<nobodywho::chat::Message>, nobodywho::errors::GetterError> {
        self.chat.get_chat_history().await
    }
}

#[flutter_rust_bridge::frb(opaque)]
pub struct NobodyWhoTokenStream {
    stream: nobodywho::chat::TokenStreamAsync,
}

impl NobodyWhoTokenStream {
    pub async fn iter(
        &mut self,
        sink: crate::frb_generated::StreamSink<String>,
    ) -> Result<(), Rust2DartSendError> {
        while let Some(token) = self.stream.next_token().await {
            sink.add(token)?;
        }
        Ok(())
    }

    pub async fn completed(&mut self) -> Result<String, nobodywho::errors::CompletionError> {
        self.stream.completed().await
    }
}

#[flutter_rust_bridge::frb(opaque)]
pub struct NobodyWhoTool {
    tool: nobodywho::chat::Tool,
}

#[flutter_rust_bridge::frb(sync)]
pub fn new_tool_impl(
    function: impl Fn(String) -> DartFnFuture<String> + Send + Sync + 'static,
    name: String,
    description: String,
    runtime_type: String,
) -> Result<NobodyWhoTool, String> {
    let json_schema = dart_function_type_to_json_schema(&runtime_type)?;

    // TODO: this seems to silently block forever if we get a type error on the dart side.
    //       it'd be *much* better to fail hard and throw a dart exception if that happens
    //       we might have to fix it on the dart side...
    let sync_callback = move |json: serde_json::Value| {
        futures::executor::block_on(async { function(json.to_string()).await })
    };

    let tool = nobodywho::chat::Tool::new(
        name,
        description,
        json_schema,
        std::sync::Arc::new(sync_callback),
    );

    Ok(NobodyWhoTool { tool })
}

/// Converts a Dart function runtimeType string directly to a JSON schema
/// Example input: "({String a, int b}) => String"
/// Returns a JSON schema for the function parameters
/// XXX: this whole function is vibe-coded, and hence the implementation is pretty messy...
#[tracing::instrument(ret, level = "debug")]
fn dart_function_type_to_json_schema(runtime_type: &str) -> Result<serde_json::Value, String> {
    // Match the pattern: ({params}) => returnType
    let re = regex::Regex::new(r"^\(\{([^}]*)\}\)\s*=>\s*(.+)$")
        .map_err(|e| format!("Regex error: {}", e))?;

    let captures = re.captures(runtime_type).ok_or_else(|| {
        if !runtime_type.contains("({") {
            format!(
                "Tool function must take only named parameters, got function type: {:?}",
                runtime_type
            )
        } else {
            "Invalid function type format".to_string()
        }
    })?;

    let params_str = &captures[1];
    let _return_type = captures[2].trim();

    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    // Parse parameters if any exist
    if !params_str.trim().is_empty() {
        for param in params_str.split(',') {
            let param = param.trim();

            // Check if parameter is required (and strip the keyword if present)
            let param_without_required = if param.starts_with("required ") {
                &param[9..] // Skip "required "
            } else {
                param
            };

            // Find the last space to split type and name
            let last_space = param_without_required
                .rfind(' ')
                .ok_or_else(|| format!("Invalid parameter format: '{}'", param))?;

            let param_type = param_without_required[..last_space].trim();
            let param_name = param_without_required[last_space + 1..].trim();

            // Convert Dart type to JSON schema type
            let schema_type = match param_type {
                "String" => serde_json::json!({ "type": "string" }),
                "int" => serde_json::json!({ "type": "integer" }),
                "double" => serde_json::json!({ "type": "number" }),
                "num" => serde_json::json!({ "type": "number" }),
                "bool" => serde_json::json!({ "type": "boolean" }),
                "DateTime" => serde_json::json!({ "type": "string", "format": "date-time" }),
                t if t.starts_with("List<") && t.ends_with('>') => {
                    let inner = &t[5..t.len() - 1];
                    let inner_schema = match inner {
                        "String" => serde_json::json!({ "type": "string" }),
                        "int" => serde_json::json!({ "type": "integer" }),
                        "double" | "num" => serde_json::json!({ "type": "number" }),
                        "bool" => serde_json::json!({ "type": "boolean" }),
                        _ => serde_json::json!({ "type": "object" }),
                    };
                    serde_json::json!({
                        "type": "array",
                        "items": inner_schema
                    })
                }
                t if t.starts_with("Map<") && t.ends_with('>') => {
                    // For simplicity, assume string keys and try to parse value type
                    let generics = &t[4..t.len() - 1];
                    let parts: Vec<&str> = generics.split(',').collect();
                    if parts.len() == 2 {
                        let value_type = parts[1].trim();
                        let value_schema = match value_type {
                            "String" => serde_json::json!({ "type": "string" }),
                            "int" => serde_json::json!({ "type": "integer" }),
                            "double" | "num" => serde_json::json!({ "type": "number" }),
                            "bool" => serde_json::json!({ "type": "boolean" }),
                            _ => serde_json::json!({ "type": "object" }),
                        };
                        serde_json::json!({
                            "type": "object",
                            "additionalProperties": value_schema
                        })
                    } else {
                        serde_json::json!({ "type": "object" })
                    }
                }
                _ => serde_json::json!({ "type": "object" }),
            };

            properties.insert(param_name.to_string(), schema_type);
            required.push(param_name.to_string());
        }
    }

    Ok(serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    }))
}

#[flutter_rust_bridge::frb(sync)]
pub fn init_debug_log() {
    // XXX: this is just for logging during dev
    // TODO: make something with configurable log levels
    //       maybe something that integrates with dart's standard logging packages
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .try_init()
        .ok();
}

// TODO:
// - blocking ask
// - embeddings
// - cross encoder
// - sampler
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dart_function_to_schema() {
        let schema = dart_function_type_to_json_schema(
            "({String name, int age, List<String> tags}) => String",
        )
        .unwrap();
        let expected = serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "integer" },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            },
            "required": ["name", "age", "tags"],
            "additionalProperties": false
        });
        assert_eq!(schema, expected);
    }

    #[test]
    fn test_empty_params() {
        let schema = dart_function_type_to_json_schema("({}) => String").unwrap();
        let expected = serde_json::json!({
            "type": "object",
            "properties": {},
            "required": [],
            "additionalProperties": false
        });
        assert_eq!(schema, expected);
    }

    #[test]
    fn test_single_string_parameter() {
        let dart_type = "({required String text}) => Future<String>";
        let json_schema = dart_function_type_to_json_schema(dart_type).unwrap();
        let expected = serde_json::json!({
            "type": "object",
            "properties": { "text": { "type": "string" } },
            "required": [ "text" ],
            "additionalProperties": false,
        });
        assert_eq!(json_schema, expected);
    }
}
