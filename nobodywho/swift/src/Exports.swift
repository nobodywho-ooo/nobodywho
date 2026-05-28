// Re-export types from NobodyWhoGenerated that are part of the public API.
//
// Uses @_exported import to make types directly available to consumers,
// rather than typealiases which don't fully resolve enum associated value
// types across module boundaries (e.g. Message.user(content:assets:) would
// fail because Asset isn't resolvable without importing NobodyWhoGenerated).

@_exported import enum NobodyWhoGenerated.Message
@_exported import enum NobodyWhoGenerated.PromptPart
@_exported import enum NobodyWhoGenerated.NobodyWhoError
@_exported import struct NobodyWhoGenerated.Asset
@_exported import struct NobodyWhoGenerated.ToolCall
@_exported import class NobodyWhoGenerated.SamplerConfig
@_exported import class NobodyWhoGenerated.SamplerBuilder

import NobodyWhoGenerated

/// Compute cosine similarity between two embedding vectors.
public func cosineSimilarity(a: [Float], b: [Float]) -> Float {
    return NobodyWhoGenerated.cosineSimilarity(a: a, b: b)
}
