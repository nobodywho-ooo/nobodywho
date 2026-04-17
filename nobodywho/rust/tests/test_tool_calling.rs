use nobodywho_rust::{Chat, Message, Model, SamplerBuilder, Tool, ToolCall};
use std::sync::Arc;

fn model_path() -> String {
    std::env::var("TEST_MODEL").unwrap_or_else(|_| "model.gguf".to_string())
}

fn load_model() -> Model {
    Model::builder(model_path())
        .build()
        .expect("failed to load model")
}

fn make_chat(model: &Model, tools: Vec<Tool>) -> Chat {
    Chat::builder(model)
        .with_system_prompt("You are a helpful assistant.")
        .with_template_variable("enable_thinking", false)
        .with_tools(tools)
        .build()
}

fn sparklify_tool() -> Tool {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "text": {"type": "string", "description": "The text to sparklify"}
        },
        "required": ["text"],
        "additionalProperties": false
    });
    Tool::new(
        "sparklify".to_string(),
        "Applies the sparklify effect to a given piece of text.".to_string(),
        schema,
        Arc::new(|args: serde_json::Value| {
            let text = args["text"].as_str().unwrap_or("");
            format!("✨{}✨", text.to_uppercase())
        }),
    )
}

fn get_weather_tool() -> Tool {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "location": {"type": "string", "description": "The city or location to get weather for"}
        },
        "required": ["location"],
        "additionalProperties": false
    });
    Tool::new(
        "get_weather".to_string(),
        "Gets the weather for a given location.".to_string(),
        schema,
        Arc::new(|args: serde_json::Value| {
            let location = args["location"].as_str().unwrap_or("unknown");
            format!("The weather in {location} is sunny and 21°C")
        }),
    )
}

fn get_tool_calls(history: &[Message]) -> Vec<ToolCall> {
    history
        .iter()
        .flat_map(|m| match m {
            Message::ToolCalls { tool_calls, .. } => tool_calls.clone(),
            _ => vec![],
        })
        .collect()
}

fn get_tool_responses(history: &[Message]) -> Vec<&Message> {
    history
        .iter()
        .filter(|m| matches!(m, Message::ToolResp { .. }))
        .collect()
}

// ── Tool construction ─────────────────────────────────────────────────────────

#[test]
fn test_tool_construction() {
    let tool = sparklify_tool();
    // invoke the tool directly to verify the callback works
    let result = (tool.function)(serde_json::json!({"text": "foobar"}));
    assert_eq!(result, "✨FOOBAR✨");
}

// ── Tool calling ──────────────────────────────────────────────────────────────

#[test]
fn test_tool_calling() {
    let model = load_model();
    let chat = make_chat(&model, vec![sparklify_tool()]);

    chat.ask("Please sparklify this word: 'julemand' and show me the result")
        .completed()
        .unwrap();

    let history = chat.get_chat_history().unwrap();
    let tool_calls = get_tool_calls(&history);
    let tool_responses = get_tool_responses(&history);

    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].name, "sparklify");
    assert_eq!(tool_calls[0].arguments["text"], "julemand");

    assert_eq!(tool_responses.len(), 1);
    if let Message::ToolResp { name, content, .. } = tool_responses[0] {
        assert_eq!(name, "sparklify");
        assert_eq!(content, "✨JULEMAND✨");
    }
}

// ── set_tools ─────────────────────────────────────────────────────────────────

#[test]
fn test_set_tools() {
    let model = load_model();
    let chat = make_chat(&model, vec![sparklify_tool()]);

    chat.ask("Please sparklify this word: 'julemand' and show me the result")
        .completed()
        .unwrap();

    let history = chat.get_chat_history().unwrap();
    let tool_calls = get_tool_calls(&history);
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].name, "sparklify");

    // swap to a different tool and clear history
    chat.set_tools(vec![get_weather_tool()]).unwrap();
    chat.reset_history().unwrap();

    chat.ask("What's the weather in Copenhagen?")
        .completed()
        .unwrap();

    let history = chat.get_chat_history().unwrap();
    let tool_calls = get_tool_calls(&history);
    let tool_responses = get_tool_responses(&history);

    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].name, "get_weather");
    assert_eq!(tool_calls[0].arguments["location"], "Copenhagen");

    assert_eq!(tool_responses.len(), 1);
    if let Message::ToolResp { content, .. } = tool_responses[0] {
        assert_eq!(content, "The weather in Copenhagen is sunny and 21°C");
    }
}

// ── Custom sampler ────────────────────────────────────────────────────────────

#[test]
fn test_tool_calling_with_custom_sampler() {
    let model = load_model();
    let sampler = SamplerBuilder::new()
        .top_k(64)
        .top_p(0.95, 2)
        .temperature(0.8)
        .dist();

    let chat = Chat::builder(&model)
        .with_system_prompt("You are a helpful assistant.")
        .with_template_variable("enable_thinking", false)
        .with_tool(sparklify_tool())
        .with_sampler(sampler)
        .build();

    chat.ask("Please sparklify this word: 'julemand' and show me the result")
        .completed()
        .unwrap();

    let history = chat.get_chat_history().unwrap();
    let tool_calls = get_tool_calls(&history);
    let tool_responses = get_tool_responses(&history);

    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].name, "sparklify");
    assert_eq!(tool_calls[0].arguments["text"], "julemand");

    assert_eq!(tool_responses.len(), 1);
    if let Message::ToolResp { content, .. } = tool_responses[0] {
        assert_eq!(content, "✨JULEMAND✨");
    }
}
