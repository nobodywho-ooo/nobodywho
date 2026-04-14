// Re-export types from NobodyWhoGenerated that are part of the public API.
// These are simple data types that don't need wrapper classes.
//
// The NobodyWhoGenerated module is NOT a product, so consumers cannot
// import it directly. Only types listed here are visible to users.

public typealias Message = NobodyWhoGenerated.Message
public typealias Role = NobodyWhoGenerated.Role
public typealias PromptPart = NobodyWhoGenerated.PromptPart
public typealias SamplerConfig = NobodyWhoGenerated.SamplerConfig
public typealias SamplerBuilder = NobodyWhoGenerated.SamplerBuilder
public typealias Asset = NobodyWhoGenerated.Asset
public typealias NobodyWhoError = NobodyWhoGenerated.NobodyWhoError

/// Compute cosine similarity between two embedding vectors.
public func cosineSimilarity(a: [Float], b: [Float]) -> Float {
    return NobodyWhoGenerated.cosineSimilarity(a: a, b: b)
}
