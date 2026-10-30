import Foundation
import NobodyWhoGenerated

/// A chat session for local LLM inference.
///
/// ```swift
/// let chat = try await Chat.fromPath(
///     modelPath: "model.gguf",
///     systemPrompt: "You are a helpful assistant."
/// )
/// for try await token in try await chat.ask("Hello!") {
///     print(token, terminator: "")
/// }
/// ```
public class Chat {
    private let inner: RustChat

    /// `threadCount` is the number of CPU threads used for inference. Leave it `nil` to
    /// detect the device's physical core count (performance cores only, on Apple silicon) —
    /// hyperthreads and efficiency cores make inference slower, not faster. Lower it to leave
    /// CPU headroom for the rest of the app.
    ///
    /// `contextShift` sets how old turns are forgotten when the context is full; `nil`
    /// uses the defaults.
    public init(
        model: Model,
        systemPrompt: String? = nil,
        contextSize: UInt32 = 4096,
        templateVariables: [String: Bool]? = nil,
        tools: [Tool]? = nil,
        sampler: SamplerConfig? = nil,
        mtp: MtpConfig? = nil,
        threadCount: UInt32? = nil,
        contextShift: ContextShiftOptions? = nil
    ) throws {
        self.inner = try RustChat(
            model: model.inner,
            systemPrompt: systemPrompt,
            contextSize: contextSize,
            templateVariables: templateVariables,
            tools: tools?.map { $0.inner },
            sampler: sampler,
            mtp: mtp,
            threadCount: threadCount,
            contextShift: contextShift
        )
    }

    /// Create a chat session directly from a model path.
    /// Loads the model and creates the chat in one step.
    public static func fromPath(
        modelPath: String,
        useGpu: Bool = true,
        projectionModelPath: String? = nil,
        draftModelPath: String? = nil,
        systemPrompt: String? = nil,
        contextSize: UInt32 = 4096,
        templateVariables: [String: Bool]? = nil,
        tools: [Tool]? = nil,
        sampler: SamplerConfig? = nil,
        mtp: MtpConfig? = nil,
        threadCount: UInt32? = nil,
        contextShift: ContextShiftOptions? = nil,
        onDownloadProgress: ((UInt64, UInt64) -> Void)? = nil
    ) async throws -> Chat {
        let model = try await Model.load(
            modelPath: modelPath,
            useGpu: useGpu,
            projectionModelPath: projectionModelPath,
            draftModelPath: draftModelPath,
            onDownloadProgress: onDownloadProgress
        )
        return try Chat(
            model: model,
            systemPrompt: systemPrompt,
            contextSize: contextSize,
            templateVariables: templateVariables,
            tools: tools,
            sampler: sampler,
            mtp: mtp,
            threadCount: threadCount,
            contextShift: contextShift
        )
    }

    /// Send a text message and get a token stream for the response.
    ///
    /// Returns after prefilling the context; throws if prefill fails.
    public func ask(_ message: String) async throws -> TokenStream {
        return TokenStream(try await inner.ask(message: message))
    }

    /// Send a multimodal prompt and get a token stream.
    ///
    /// Returns after prefilling the context; throws if prefill fails.
    public func ask(_ prompt: Prompt) async throws -> TokenStream {
        if let json = prompt.jsonString {
            return TokenStream(try await inner.askWithJsonPrompt(json: json))
        }
        return TokenStream(try await inner.askWithPrompt(parts: prompt.parts!))
    }

    /// Answer a full list of messages, replacing the chat history.
    ///
    /// The list is the whole conversation, used as given: it must be non-empty and
    /// end in a user or tool message. A leading system message sets the chat's system
    /// prompt; leave it out and the prompt already on the chat is kept. A later one
    /// stays in the history, for the chat template to render in place. The response
    /// is appended, and the next `ask` continues from there.
    ///
    /// `options` follows the same rule for the chat's other settings.
    ///
    /// Returns after prefilling the context; throws if the list is invalid or
    /// prefill fails.
    public func complete(_ messages: [Message], options: Options = Options()) async throws -> TokenStream {
        return TokenStream(try await inner.complete(messages: messages, options: options.toGenerated()))
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
    public func getChatHistory() async -> [Message] {
        return await inner.getChatHistory()
    }

    /// Set the chat history from a list of messages.
    public func setChatHistory(_ messages: [Message]) async throws {
        try await inner.setChatHistory(messages: messages)
    }

    /// Get the current system prompt.
    public func getSystemPrompt() async -> String? {
        return await inner.getSystemPrompt()
    }

    /// Set the system prompt.
    public func setSystemPrompt(_ systemPrompt: String?) async {
        await inner.setSystemPrompt(systemPrompt: systemPrompt)
    }

    /// Set how old turns are forgotten when the context is full.
    public func setContextShift(_ options: ContextShiftOptions) async throws {
        try await inner.setContextShift(options: options)
    }

    /// Set the tools available to the model.
    public func setTools(_ tools: [Tool]) async throws {
        try await inner.setTools(tools: tools.map { $0.inner })
    }

    /// Set a template variable.
    public func setTemplateVariable(name: String, value: Bool) async {
        await inner.setTemplateVariable(name: name, value: value)
    }

    /// Get all template variables.
    public func getTemplateVariables() async -> [String: Bool] {
        return await inner.getTemplateVariables()
    }

    /// Set the sampler configuration.
    public func setSamplerConfig(_ sampler: SamplerConfig) async throws {
        try await inner.setSamplerConfig(sampler: sampler)
    }

    /// Get the current sampler configuration as a JSON string.
    public func getSamplerConfigJson() async throws -> String {
        return try await inner.getSamplerConfigJson()
    }

    /// Get context usage statistics.
    public func getStats() async -> ChatStats {
        return await inner.getStats()
    }

    /// MTP draft acceptance rate for the most recent generation, or nil if unavailable.
    public func mtpAcceptanceRate() async -> Float? {
        return await inner.mtpAcceptanceRate()
    }

    /// Tokenize a message using the model's tokenizer.
    public func tokenize(message: String) async throws -> [Int32?] {
        return try await inner.tokenize(message: message)
    }
}
