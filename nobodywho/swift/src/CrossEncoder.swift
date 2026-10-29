import Foundation
import NobodyWhoGenerated

/// A cross-encoder for ranking documents by relevance to a query.
///
/// ```swift
/// let crossEncoder = try await CrossEncoder.fromPath(modelPath: "reranker-model.gguf")
/// let scores = try await crossEncoder.rank(query: "How to reset password?", documents: docs)
/// ```
public class CrossEncoder {
    private let inner: NobodyWhoGenerated.RustCrossEncoder

    /// Create a cross-encoder from a loaded model, blocking the calling thread
    /// until it is ready. Prefer ``load(model:contextSize:)`` from async code.
    ///
    /// - Parameters:
    ///   - model: A loaded model to use for ranking.
    ///   - contextSize: Maximum context size in tokens. Defaults to 4096 if nil.
    /// - Throws: If the context cannot be created.
    public init(model: Model, contextSize: UInt32? = nil) throws {
        self.inner = try NobodyWhoGenerated.RustCrossEncoder(model: model.inner, contextSize: contextSize)
    }

    private init(inner: NobodyWhoGenerated.RustCrossEncoder) {
        self.inner = inner
    }

    /// Create a cross-encoder from a loaded model asynchronously.
    ///
    /// - Parameters:
    ///   - model: A loaded model to use for ranking.
    ///   - contextSize: Maximum context size in tokens. Defaults to 4096 if nil.
    /// - Throws: If the context cannot be created.
    public static func load(model: Model, contextSize: UInt32? = nil) async throws -> CrossEncoder {
        let inner = try await NobodyWhoGenerated.loadCrossEncoder(model: model.inner, contextSize: contextSize)
        return CrossEncoder(inner: inner)
    }

    /// Create a cross-encoder directly from a model path.
    public static func fromPath(
        modelPath: String,
        useGpu: Bool = true,
        contextSize: UInt32? = nil
    ) async throws -> CrossEncoder {
        let model = try await Model.load(modelPath: modelPath, useGpu: useGpu)
        return try await CrossEncoder.load(model: model, contextSize: contextSize)
    }

    /// Rank documents by relevance to a query. Returns similarity scores.
    public func rank(query: String, documents: [String]) async throws -> [Float] {
        return try await inner.rank(query: query, documents: documents)
    }

    /// Rank documents and return them sorted by relevance (most relevant first).
    /// Returns an array of (document, score) tuples.
    public func rankAndSort(query: String, documents: [String]) async throws -> [(String, Float)] {
        let jsonResult = try await inner.rankAndSortJson(query: query, documents: documents)
        // Rust serializes Vec<(String, f32)> as [["doc", 0.95], ...]
        guard let data = jsonResult.data(using: .utf8),
              let parsed = try? JSONSerialization.jsonObject(with: data) as? [[Any]] else {
            return []
        }
        return parsed.compactMap { pair in
            guard pair.count == 2,
                  let doc = pair[0] as? String,
                  let score = pair[1] as? NSNumber else { return nil }
            return (doc, score.floatValue)
        }
    }
}
