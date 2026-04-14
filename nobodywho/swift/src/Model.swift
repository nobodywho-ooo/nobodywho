import Foundation
import NobodyWhoGenerated

/// A loaded GGUF model that can be shared between multiple Chat, Encoder,
/// or CrossEncoder instances.
///
/// ```swift
/// let model = try await Model.load(modelPath: "model.gguf")
/// let chat1 = Chat(model: model)
/// let chat2 = Chat(model: model)
/// ```
public class Model {
    let inner: NobodyWhoGenerated.Model

    init(inner: NobodyWhoGenerated.Model) {
        self.inner = inner
    }

    /// Load a GGUF model from disk.
    ///
    /// - Parameters:
    ///   - modelPath: Path to the .gguf model file.
    ///   - useGpu: Enable GPU acceleration (default: true).
    ///   - imageModelPath: Optional path to an mmproj file for vision models.
    public static func load(
        modelPath: String,
        useGpu: Bool = true,
        imageModelPath: String? = nil
    ) async throws -> Model {
        let inner = try await NobodyWhoGenerated.loadModel(
            modelPath: modelPath,
            useGpu: useGpu,
            imageModelPath: imageModelPath
        )
        return Model(inner: inner)
    }
}
