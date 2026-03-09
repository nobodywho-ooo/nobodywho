package com.nobodywho

import java.util.UUID

/** Coordinates an embedder and a vector store to provide semantic search over documents. */
class SemanticMemory(
    private val embedder: EmbedderAgent,
    private val vectorStore: VectorStore
) {
    /** Embeds and stores a document. */
    fun saveDocument(id: String, text: String, metadata: Map<String, String> = emptyMap()) {
        val vector = embedder.embed(text)
        val fullMetadata = metadata.toMutableMap().also { it["text"] = text }
        vectorStore.add(id = id, vector = vector, metadata = fullMetadata)
    }

    /** Embeds and stores multiple documents. */
    fun saveDocuments(documents: List<Triple<String, String, Map<String, String>>>) {
        for ((id, text, metadata) in documents) {
            saveDocument(id = id, text = text, metadata = metadata)
        }
    }

    /** Returns the top-K documents most semantically similar to the query. */
    fun search(query: String, topK: Int = 3): List<ScoredDocument> {
        val queryVector = embedder.embed(query)
        return vectorStore.search(query = queryVector, topK = topK)
    }

    fun removeDocument(id: String) = vectorStore.remove(id)

    fun clear() = vectorStore.clear()

    val documentCount: Int get() = vectorStore.count
}

/**
 * Extends `SemanticMemory` with an optional cross-encoder reranker for two-stage retrieval.
 *
 * Vector search narrows candidates to `rerankCandidates`, then the cross-encoder rescores
 * and returns the top-K results.
 */
class HybridSemanticMemory(
    embedder: EmbedderAgent,
    vectorStore: VectorStore,
    private val reranker: Reranker? = null
) {
    private val semanticMemory = SemanticMemory(embedder = embedder, vectorStore = vectorStore)

    /** Embeds and stores a document. */
    fun saveDocument(id: String, text: String, metadata: Map<String, String> = emptyMap()) {
        semanticMemory.saveDocument(id = id, text = text, metadata = metadata)
    }

    /**
     * Returns the top-K documents.
     *
     * When a reranker is configured, fetches `rerankCandidates` via vector search,
     * then rescores with a cross-encoder and returns the top-K results.
     */
    fun search(query: String, topK: Int = 3, rerankCandidates: Int = 10): List<ScoredDocument> {
        if (reranker == null) {
            return semanticMemory.search(query = query, topK = topK)
        }

        val candidates = semanticMemory.search(query = query, topK = rerankCandidates)
        val reranked = reranker.rankAndSort(query = query, documents = candidates.map { it.text })

        return reranked.take(topK).map { ranked ->
            val original = candidates.firstOrNull { it.text == ranked.content }
            ScoredDocument(
                id = original?.id ?: UUID.randomUUID().toString(),
                text = ranked.content,
                score = ranked.score,
                metadata = original?.metadata ?: emptyMap()
            )
        }
    }

    fun clear() = semanticMemory.clear()

    val documentCount: Int get() = semanticMemory.documentCount
}
