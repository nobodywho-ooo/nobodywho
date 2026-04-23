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

    public init(model: Model, contextSize: UInt32 = 4096) {
        self.inner = NobodyWhoGenerated.RustCrossEncoder(model: model.inner, contextSize: contextSize)
    }

    /// Create a cross-encoder directly from a model path.
    public static func fromPath(
        modelPath: String,
        useGpu: Bool = true,
        contextSize: UInt32 = 4096
    ) async throws -> CrossEncoder {
        let model = try await Model.load(modelPath: modelPath, useGpu: useGpu)
        return CrossEncoder(model: model, contextSize: contextSize)
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
