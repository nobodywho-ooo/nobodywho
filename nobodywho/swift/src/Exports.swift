// Re-export types from NobodyWhoGenerated that are part of the public API.
//
// Uses @_exported import to make types directly available to consumers,
// rather than typealiases which don't fully resolve enum associated value
// types across module boundaries (e.g. Message.user(content:) would fail
// because MessageContent isn't resolvable without importing NobodyWhoGenerated).

@_exported import enum NobodyWhoGenerated.Message
@_exported import enum NobodyWhoGenerated.ContentPart
@_exported import enum NobodyWhoGenerated.MessageContent
@_exported import enum NobodyWhoGenerated.NobodyWhoError
@_exported import enum NobodyWhoGenerated.VoiceActivityDetectionEvent
@_exported import struct NobodyWhoGenerated.ToolCall
@_exported import class NobodyWhoGenerated.SamplerConfig
@_exported import class NobodyWhoGenerated.SamplerBuilder
@_exported import struct NobodyWhoGenerated.CachedModel

import NobodyWhoGenerated

/// Settings to apply before a `complete` turn. `nil` keeps what the chat has; a
/// set value stays set, like a leading system message.
public struct Options {
    public var sampler: SamplerConfig?
    /// Replaces the chat's template variables wholesale.
    public var templateVariables: [String: Bool]?
    /// Re-selects the chat template, so the turn re-prefills from near token zero.
    public var tools: [Tool]?

    public init(
        sampler: SamplerConfig? = nil,
        templateVariables: [String: Bool]? = nil,
        tools: [Tool]? = nil
    ) {
        self.sampler = sampler
        self.templateVariables = templateVariables
        self.tools = tools
    }

    func toGenerated() -> NobodyWhoGenerated.Options {
        NobodyWhoGenerated.Options(
            sampler: sampler,
            templateVariables: templateVariables,
            tools: tools?.map { $0.inner }
        )
    }
}

/// Shorthands for the common case of text-only content.
extension Message {
    public static func user(_ text: String) -> Message {
        .user(content: .text(text: text))
    }

    public static func assistant(_ text: String, toolCalls: [ToolCall]? = nil) -> Message {
        .assistant(content: .text(text: text), toolCalls: toolCalls)
    }

    public static func system(_ text: String) -> Message {
        .system(content: .text(text: text))
    }

    public static func tool(name: String, _ text: String) -> Message {
        .tool(name: name, content: .text(text: text))
    }
}

extension MessageContent {
    /// This content as text.
    ///
    /// Media parts are left out, so for content that interleaves text and media
    /// this is only the text around it — switch on the content itself when the
    /// media matters.
    public var text: String {
        switch self {
        case .text(let text):
            return text
        case .json(let json):
            return json
        case .parts(let parts):
            return parts.compactMap {
                if case .text(let text) = $0 { return text } else { return nil }
            }.joined()
        }
    }
}

/// Compute cosine similarity between two embedding vectors.
public func cosineSimilarity(a: [Float], b: [Float]) -> Float {
    return NobodyWhoGenerated.cosineSimilarity(a: a, b: b)
}

/// Returns every cached `.gguf` model paired with its byte size.
///
/// Scans the platform model cache directory. Returns an empty array if the cache
/// directory does not exist yet.
public func getCachedModels() throws -> [CachedModel] {
    return try NobodyWhoGenerated.getCachedModels()
}
