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
    ///   - modelPath: Local path, `hf://` or `https://` URL, or `auto` for memory-based model selection.
    ///   - useGpu: Enable GPU acceleration (default: true).
    ///   - projectionModelPath: Optional path to an mmproj file for vision models.
    ///   - draftModelPath: Optional path to an MTP draft-heads gguf. Loading it
    ///     lets `Chat` instances opt into MTP speculative decoding via
    ///     `Chat(..., mtp: true)`. Adds around 5% to VRAM usage.
    ///   - onDownloadProgress: Optional callback receiving `(downloadedBytes, totalBytes)` during download.
    public static func load(
        modelPath: String,
        useGpu: Bool = true,
        projectionModelPath: String? = nil,
        draftModelPath: String? = nil,
        onDownloadProgress: ((UInt64, UInt64) -> Void)? = nil
    ) async throws -> Model {
        let callback = onDownloadProgress.map { DownloadProgressCallbackImpl($0) }
        let inner = try await NobodyWhoGenerated.loadModel(
            modelPath: modelPath,
            useGpu: useGpu,
            projectionModelPath: projectionModelPath,
            draftModelPath: draftModelPath,
            onDownloadProgress: callback
        )
        return Model(inner: inner)
    }

    /// The maximum context size this model was trained with.
    /// Use this to avoid setting `contextSize` above what the model supports.
    public var maxCtx: UInt32 {
        inner.maxCtx()
    }

    /// Download a GGUF model from a remote URL or HuggingFace path and return the local file path.
    ///
    /// Use this when you need custom HTTP headers, e.g. for gated models that require
    /// authentication. For unauthenticated downloads, pass the URL directly to `load`.
    ///
    /// - Parameters:
    ///   - modelPath: Path or URL to a GGUF model file (`hf://owner/repo/file.gguf`, `https://`, or a local path).
    ///   - headers: Optional HTTP headers (e.g. `["Authorization": "Bearer hf_..."]`).
    ///   - onDownloadProgress: Optional callback receiving `(downloadedBytes, totalBytes)` during download.
    /// - Returns: The local filesystem path where the model was cached.
    public static func downloadModel(
        modelPath: String,
        headers: [String: String]? = nil,
        onDownloadProgress: ((UInt64, UInt64) -> Void)? = nil
    ) async throws -> String {
        let callback = onDownloadProgress.map { DownloadProgressCallbackImpl($0) }
        return try await NobodyWhoGenerated.downloadModel(
            modelPath: modelPath,
            headers: headers,
            onDownloadProgress: callback
        )
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
