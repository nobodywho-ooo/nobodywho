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

    /// Load a GGUF model from disk or remote URL.
    ///
    /// - Parameters:
    ///   - modelPath: Path to the .gguf model file, `hf://owner/repo/file.gguf`, or an `https://` URL.
    ///   - useGpu: Enable GPU acceleration (default: true).
    ///   - projectionModelPath: Optional path to an mmproj file for vision models.
    ///   - onDownloadProgress: Optional callback receiving `(downloadedBytes, totalBytes)` during download.
    public static func load(
        modelPath: String,
        useGpu: Bool = true,
        projectionModelPath: String? = nil,
        onDownloadProgress: ((UInt64, UInt64) -> Void)? = nil
    ) async throws -> Model {
        let callback = onDownloadProgress.map { DownloadProgressCallbackImpl($0) }
        let inner = try await NobodyWhoGenerated.loadModel(
            modelPath: modelPath,
            useGpu: useGpu,
            projectionModelPath: projectionModelPath,
            onDownloadProgress: callback
        )
        return Model(inner: inner)
    }
}

/// Bridges a Swift closure to the `RustDownloadProgressCallback` protocol.
private final class DownloadProgressCallbackImpl: @unchecked Sendable, RustDownloadProgressCallback {
    let handler: (UInt64, UInt64) -> Void

    init(_ handler: @escaping (UInt64, UInt64) -> Void) {
        self.handler = handler
    }

    func onDownloadProgress(downloaded: UInt64, total: UInt64) {
        handler(downloaded, total)
    }
}
