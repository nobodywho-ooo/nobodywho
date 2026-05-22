package ai.nobodywho

import java.io.Closeable
import uniffi.nobodywho.RustModel as InternalModel
import uniffi.nobodywho.loadModel
import uniffi.nobodywho.downloadModel as internalDownloadModel
import uniffi.nobodywho.RustDownloadProgressCallback

/**
 * A loaded LLM model that can be shared across Chat, Encoder, and CrossEncoder instances.
 *
 * ```kotlin
 * val model = Model.load("model.gguf")
 * val chat = Chat(model = model, systemPrompt = "You are a helpful assistant.")
 * ```
 */
class Model internal constructor(
    internal val inner: InternalModel
) : Closeable {
    companion object {
        /**
         * Load a GGUF model from a local path or remote URL.
         *
         * @param modelPath Path to the .gguf file, `hf://owner/repo/file.gguf`, or an `https://` URL.
         * @param useGpu Enable GPU acceleration (default: true).
         * @param projectionModelPath Optional path to an mmproj file for vision models.
         * @param onDownloadProgress Optional callback receiving (downloadedBytes, totalBytes) during download.
         */
        suspend fun load(
            modelPath: String,
            useGpu: Boolean = true,
            projectionModelPath: String? = null,
            onDownloadProgress: ((downloaded: ULong, total: ULong) -> Unit)? = null
        ): Model {
            val callback = onDownloadProgress?.let { cb ->
                object : RustDownloadProgressCallback {
                    override fun onDownloadProgress(downloaded: ULong, total: ULong) = cb(downloaded, total)
                }
            }
            return Model(loadModel(modelPath, useGpu, projectionModelPath, callback))
        }

        /**
         * Download a model from a remote URL or HuggingFace path and return the local file path.
         *
         * Use this when you need custom headers, e.g. for gated models that require authentication.
         * For unauthenticated downloads, pass the URL directly to [load].
         *
         * @param modelPath `hf://owner/repo/file.gguf` or an `https://` URL.
         * @param headers Optional HTTP headers (e.g. `mapOf("Authorization" to "Bearer token")`).
         * @param onDownloadProgress Optional callback receiving (downloadedBytes, totalBytes).
         */
        suspend fun download(
            modelPath: String,
            headers: Map<String, String>? = null,
            onDownloadProgress: ((downloaded: ULong, total: ULong) -> Unit)? = null
        ): String {
            val callback = onDownloadProgress?.let { cb ->
                object : RustDownloadProgressCallback {
                    override fun onDownloadProgress(downloaded: ULong, total: ULong) = cb(downloaded, total)
                }
            }
            return internalDownloadModel(modelPath, headers, callback)
        }
    }

    /** Free the underlying Rust resources. */
    fun destroy() = inner.destroy()
    override fun close() { destroy() }
}
