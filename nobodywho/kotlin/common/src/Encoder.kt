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
class Encoder private constructor(
    private val inner: InternalEncoder
) : Closeable {
    companion object {
        /** Create from a loaded model asynchronously. */
        suspend fun create(model: Model, contextSize: UInt? = null): Encoder =
            Encoder(InternalEncoder.create(model.inner, contextSize))

        /** Create an encoder directly from a model path. */
        suspend fun fromPath(
            modelPath: String,
            useGpu: Boolean = true,
            contextSize: UInt? = null
        ): Encoder = create(Model.load(modelPath, useGpu), contextSize)
    }

    suspend fun encode(text: String): List<Float> = inner.encode(text)

    /** Encode multiple texts, preserving input order. */
    suspend fun encodeBatch(texts: List<String>): List<List<Float>> = inner.encodeBatch(texts)

    /** Free the underlying Rust resources. */
    fun destroy() = inner.destroy()
    override fun close() { destroy() }
}
