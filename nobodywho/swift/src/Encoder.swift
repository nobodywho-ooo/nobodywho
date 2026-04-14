import Foundation
import NobodyWhoGenerated

/// An encoder for generating text embeddings.
///
/// ```swift
/// let encoder = try await Encoder.fromPath(modelPath: "embedding-model.gguf")
/// let embedding = try await encoder.encode("Hello world")
/// ```
public class Encoder {
    private let inner: NobodyWhoGenerated.Encoder

    public init(model: Model, contextSize: UInt32 = 4096) {
        self.inner = NobodyWhoGenerated.Encoder(model: model.inner, contextSize: contextSize)
    }

    /// Create an encoder directly from a model path.
    public static func fromPath(
        modelPath: String,
        useGpu: Bool = true,
        contextSize: UInt32 = 4096
    ) async throws -> Encoder {
        let model = try await Model.load(modelPath: modelPath, useGpu: useGpu)
        return Encoder(model: model, contextSize: contextSize)
    }

    /// Encode text into an embedding vector.
    public func encode(_ text: String) async throws -> [Float] {
        return try await inner.encode(text: text)
    }
}
