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
    let inner: NobodyWhoGenerated.RustModel

    init(inner: NobodyWhoGenerated.RustModel) {
        self.inner = inner
    }

    /// Load a GGUF model from disk.
    ///
    /// - Parameters:
    ///   - modelPath: Path to the .gguf model file.
    ///   - useGpu: Enable GPU acceleration (default: true).
    ///   - projectionModelPath: Optional path to an mmproj file for vision models.
    public static func load(
        modelPath: String,
        useGpu: Bool = true,
        projectionModelPath: String? = nil
    ) async throws -> Model {
        let inner = try await NobodyWhoGenerated.loadModel(
            modelPath: modelPath,
            useGpu: useGpu,
            projectionModelPath: projectionModelPath
        )
        return Model(inner: inner)
    }
}
