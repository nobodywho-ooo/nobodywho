import Foundation
import NobodyWhoGenerated

/// A chat session for local LLM inference.
///
/// ```swift
/// let chat = try await Chat.fromPath(
///     modelPath: "model.gguf",
///     systemPrompt: "You are a helpful assistant."
/// )
/// for await token in chat.ask("Hello!") {
///     print(token, terminator: "")
/// }
/// ```
public class Chat {
    private let inner: RustChat

    public init(
        model: Model,
        systemPrompt: String? = nil,
        contextSize: UInt32 = 4096,
        templateVariables: [String: Bool]? = nil,
        tools: [Tool]? = nil,
        sampler: SamplerConfig? = nil
    ) {
        self.inner = RustChat(
            model: model.inner,
            systemPrompt: systemPrompt,
            contextSize: contextSize,
            templateVariables: templateVariables,
            tools: tools?.map { $0.inner },
            sampler: sampler
        )
    }

    /// Create a chat session directly from a model path.
    /// Loads the model and creates the chat in one step.
    public static func fromPath(
        modelPath: String,
        useGpu: Bool = true,
        imageModelPath: String? = nil,
        systemPrompt: String? = nil,
        contextSize: UInt32 = 4096,
        templateVariables: [String: Bool]? = nil,
        tools: [Tool]? = nil,
        sampler: SamplerConfig? = nil
    ) async throws -> Chat {
        let model = try await Model.load(
            modelPath: modelPath,
            useGpu: useGpu,
            imageModelPath: imageModelPath
        )
        return Chat(
            model: model,
            systemPrompt: systemPrompt,
            contextSize: contextSize,
            templateVariables: templateVariables,
            tools: tools,
            sampler: sampler
        )
    }

    /// Send a text message and get a token stream for the response.
    public func ask(_ message: String) -> TokenStream {
        return TokenStream(inner.ask(message: message))
    }

    /// Send a multimodal prompt and get a token stream.
    public func ask(_ prompt: Prompt) -> TokenStream {
        return TokenStream(inner.askWithPrompt(parts: prompt.parts))
    }

    /// Stop the current generation.
    public func stopGeneration() {
        inner.stopGeneration()
    }

    /// Reset the chat context with a new system prompt and tools.
    public func resetContext(systemPrompt: String? = nil, tools: [Tool]? = nil) async throws {
        try await inner.resetContext(
            systemPrompt: systemPrompt,
            tools: tools?.map { $0.inner }
        )
    }

    /// Reset the chat history, keeping the system prompt and tools.
    public func resetHistory() async throws {
        try await inner.resetHistory()
    }

    /// Get the current chat history as a list of messages.
    public func getChatHistory() async throws -> [Message] {
        return try await inner.getChatHistory()
    }

    /// Set the chat history from a list of messages.
    public func setChatHistory(_ messages: [Message]) async throws {
        try await inner.setChatHistory(messages: messages)
    }

    /// Get the current system prompt.
    public func getSystemPrompt() async throws -> String? {
        return try await inner.getSystemPrompt()
    }

    /// Set the system prompt.
    public func setSystemPrompt(_ systemPrompt: String?) async throws {
        try await inner.setSystemPrompt(systemPrompt: systemPrompt)
    }

    /// Set the tools available to the model.
    public func setTools(_ tools: [Tool]) async throws {
        try await inner.setTools(tools: tools.map { $0.inner })
    }

    /// Set a template variable.
    public func setTemplateVariable(name: String, value: Bool) async throws {
        try await inner.setTemplateVariable(name: name, value: value)
    }

    /// Get all template variables.
    public func getTemplateVariables() async throws -> [String: Bool] {
        return try await inner.getTemplateVariables()
    }

    /// Set the sampler configuration.
    public func setSamplerConfig(_ sampler: SamplerConfig) async throws {
        try await inner.setSamplerConfig(sampler: sampler)
    }

    /// Get the current sampler configuration as a JSON string.
    public func getSamplerConfigJson() async throws -> String {
        return try await inner.getSamplerConfigJson()
    }
}
