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

// Re-export all public types from the generated UniFFI bindings
@_exported import struct Foundation.Date
@_exported import struct Foundation.Data
@_exported import class Foundation.NSObject

// The generated UniFFI bindings are in Generated/NobodyWhoFFI.swift
// and are automatically compiled as part of this module.
// All public types (Chat, Model, ChatConfig, etc.) are available when you import NobodyWho.
