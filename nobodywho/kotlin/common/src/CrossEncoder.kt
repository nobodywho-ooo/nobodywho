package ai.nobodywho

import java.io.Closeable
import org.json.JSONArray
import uniffi.nobodywho.RustCrossEncoder as InternalCrossEncoder
import uniffi.nobodywho.loadCrossEncoder

/**
 * A cross-encoder for ranking documents by relevance to a query.
 *
 * ```kotlin
 * val crossEncoder = CrossEncoder.fromPath("crossencoder.gguf")
 * val scores = crossEncoder.rank("query", listOf("doc1", "doc2"))
 * val sorted = crossEncoder.rankAndSort("query", listOf("doc1", "doc2"))
 * ```
 */
class CrossEncoder private constructor(
    private val inner: InternalCrossEncoder
) : Closeable {
    /** Create synchronously from a loaded model, blocking until it is ready. Prefer [load] from coroutines. */
    constructor(model: Model, contextSize: UInt? = null) : this(InternalCrossEncoder(model.inner, contextSize))

    companion object {
        /** Create from a loaded model asynchronously. */
        suspend fun load(model: Model, contextSize: UInt? = null): CrossEncoder =
            CrossEncoder(loadCrossEncoder(model.inner, contextSize))

        /** Create a cross-encoder directly from a model path. */
        suspend fun fromPath(
            modelPath: String,
            useGpu: Boolean = true,
            contextSize: UInt? = null
        ): CrossEncoder = load(Model.load(modelPath, useGpu), contextSize)
    }

    suspend fun rank(query: String, documents: List<String>): List<Float> =
        inner.rank(query, documents)

    /** Rank and return documents sorted by descending relevance as (document, score) pairs. */
    suspend fun rankAndSort(query: String, documents: List<String>): List<Pair<String, Float>> {
        val array = JSONArray(inner.rankAndSortJson(query, documents))
        return (0 until array.length()).map { i ->
            val pair = array.getJSONArray(i)
            Pair(pair.getString(0), pair.getDouble(1).toFloat())
        }
    }

    /** Free the underlying Rust resources. */
    fun destroy() = inner.destroy()
    override fun close() { destroy() }
}
