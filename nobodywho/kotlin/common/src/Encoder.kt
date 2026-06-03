package ai.nobodywho

import java.io.Closeable
import uniffi.nobodywho.RustEncoder as InternalEncoder

/**
 * An embedding encoder for generating text embeddings.
 *
 * ```kotlin
 * val encoder = Encoder.fromPath("embeddings.gguf")
 * val embedding = encoder.encode("Hello world")
 * ```
 */
class Encoder(
    model: Model,
    contextSize: UInt? = null
) : Closeable {
    private val inner: InternalEncoder = InternalEncoder(model.inner, contextSize)

    companion object {
        /** Create an encoder directly from a model path. */
        suspend fun fromPath(
            modelPath: String,
            useGpu: Boolean = true,
            contextSize: UInt? = null
        ): Encoder = Encoder(Model.load(modelPath, useGpu), contextSize)
    }

    suspend fun encode(text: String): List<Float> = inner.encode(text)

    /** Free the underlying Rust resources. */
    fun destroy() = inner.destroy()
    override fun close() { destroy() }
}
