import Foundation
import NobodyWhoGenerated

/// An encoder for generating text embeddings.
///
/// ```swift
/// let encoder = try await Encoder.fromPath(modelPath: "embedding-model.gguf")
/// let embedding = try await encoder.encode("Hello world")
/// ```
public class Encoder {
    private let inner: NobodyWhoGenerated.RustEncoder

    /// Create an encoder from a loaded model, blocking the calling thread
    /// until it is ready. Prefer ``load(model:contextSize:)`` from async code.
    ///
    /// - Parameters:
    ///   - model: A loaded model to use for encoding.
    ///   - contextSize: Maximum context size in tokens. Defaults to 4096 if nil.
    /// - Throws: If the context cannot be created.
    public init(model: Model, contextSize: UInt32? = nil) throws {
        self.inner = try NobodyWhoGenerated.RustEncoder(model: model.inner, contextSize: contextSize)
    }

    private init(inner: NobodyWhoGenerated.RustEncoder) {
        self.inner = inner
    }

    /// Create an encoder from a loaded model asynchronously.
    ///
    /// - Parameters:
    ///   - model: A loaded model to use for encoding.
    ///   - contextSize: Maximum context size in tokens. Defaults to 4096 if nil.
    /// - Throws: If the context cannot be created.
    public static func load(model: Model, contextSize: UInt32? = nil) async throws -> Encoder {
        let inner = try await NobodyWhoGenerated.loadEncoder(model: model.inner, contextSize: contextSize)
        return Encoder(inner: inner)
    }

    /// Create an encoder directly from a model path.
    public static func fromPath(
        modelPath: String,
        useGpu: Bool = true,
        contextSize: UInt32? = nil
    ) async throws -> Encoder {
        let model = try await Model.load(modelPath: modelPath, useGpu: useGpu)
        return try await Encoder.load(model: model, contextSize: contextSize)
    }

    /// Encode text into an embedding vector.
    public func encode(_ text: String) async throws -> [Float] {
        return try await inner.encode(text: text)
    }

    /// Encode multiple texts, preserving input order.
    public func encodeBatch(_ texts: [String]) async throws -> [[Float]] {
        return try await inner.encodeBatch(texts: texts)
    }
}
