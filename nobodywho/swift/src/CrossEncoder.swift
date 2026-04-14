import Foundation
import NobodyWhoGenerated

/// A cross-encoder for ranking documents by relevance to a query.
///
/// ```swift
/// let crossEncoder = try await CrossEncoder.fromPath(modelPath: "reranker-model.gguf")
/// let scores = try await crossEncoder.rank(query: "How to reset password?", documents: docs)
/// ```
public class CrossEncoder {
    private let inner: NobodyWhoGenerated.CrossEncoder

    public init(model: Model, contextSize: UInt32 = 4096) {
        self.inner = NobodyWhoGenerated.CrossEncoder(model: model.inner, contextSize: contextSize)
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
        // Parse the JSON result into tuples
        guard let data = jsonResult.data(using: .utf8),
              let parsed = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] else {
            return []
        }
        return parsed.compactMap { entry in
            guard let doc = entry["document"] as? String,
                  let score = entry["score"] as? NSNumber else { return nil }
            return (doc, score.floatValue)
        }
    }
}
