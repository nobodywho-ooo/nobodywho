package ai.nobodywho

import java.io.Closeable
import uniffi.nobodywho.RustEncoder as InternalEncoder
import uniffi.nobodywho.loadEncoder

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
    /** Create synchronously from a loaded model, blocking until it is ready. Prefer [load] from coroutines. */
    constructor(model: Model, contextSize: UInt? = null) : this(InternalEncoder(model.inner, contextSize))

    companion object {
        /** Create from a loaded model asynchronously. */
        suspend fun load(model: Model, contextSize: UInt? = null): Encoder =
            Encoder(loadEncoder(model.inner, contextSize))

        /** Create an encoder directly from a model path. */
        suspend fun fromPath(
            modelPath: String,
            useGpu: Boolean = true,
            contextSize: UInt? = null
        ): Encoder = load(Model.load(modelPath, useGpu), contextSize)
    }

    suspend fun encode(text: String): List<Float> = inner.encode(text)

    /** Encode multiple texts, preserving input order. */
    suspend fun encodeBatch(texts: List<String>): List<List<Float>> = inner.encodeBatch(texts)

    /** Free the underlying Rust resources. */
    fun destroy() = inner.destroy()
    override fun close() { destroy() }
}
