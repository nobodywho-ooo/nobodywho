/// NobodyWho Swift SDK
///
/// A Swift wrapper around the NobodyWho Rust library for running LLMs locally on iOS and macOS.
///
/// # Example Usage
///
/// ```swift
/// import NobodyWho
///
/// // Load a model
/// let model = try loadModel(path: "/path/to/model.gguf", useGpu: true)
///
/// // Create a chat
/// let config = ChatConfig(contextSize: 4096, systemPrompt: "You are a helpful assistant")
/// let chat = try Chat(model: model, config: config)
///
/// // Ask a question
/// let response = try chat.askBlocking(prompt: "What is the capital of France?")
/// print(response)
///
/// // Get chat history
/// let history = try chat.history()
/// for message in history {
///     print("\(message.role): \(message.content)")
/// }
/// ```

// The generated UniFFI bindings (NobodyWhoFFI.swift) are automatically
// included in this module and provide the following public API:
//
// - Chat: Main chat interface
// - Model: Loaded GGUF model
// - ChatConfig: Configuration for chat sessions
// - Message: Chat message type
// - Role: Message role (user, assistant, system, tool)
// - NobodyWhoError: Error types
// - loadModel(path:useGpu:): Load a GGUF model
// - initLogging(): Initialize logging
