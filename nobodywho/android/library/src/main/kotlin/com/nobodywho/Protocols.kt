package com.nobodywho

import kotlinx.coroutines.flow.Flow

// MARK: - Agent Protocols

/** Converts text into embedding vectors. */
interface EmbedderAgent {
    fun embed(text: String): FloatArray
    fun embedBatch(texts: List<String>): List<FloatArray>
}

/** Stores and retrieves vectors by cosine similarity. */
interface VectorStore {
    fun add(id: String, vector: FloatArray, metadata: Map<String, String>)
    fun search(query: FloatArray, topK: Int): List<ScoredDocument>
    fun remove(id: String)
    fun clear()
    val count: Int
}

/** Reranks documents by relevance to a query using a cross-encoder. */
interface Reranker {
    fun rank(query: String, documents: List<String>): FloatArray
    fun rankAndSort(query: String, documents: List<String>): List<RankedResult>
}

/** Generates text from a prompt. */
interface LanguageModel {
    suspend fun generate(prompt: String): String
    fun generateFlow(prompt: String): Flow<String>
}

// MARK: - Data Types

/** A document returned from a vector search, with its relevance score. */
data class ScoredDocument(
    val id: String,
    val text: String,
    val score: Float,
    val metadata: Map<String, String> = emptyMap()
)

/** A document with its cross-encoder relevance score. */
data class RankedResult(
    val content: String,
    val score: Float
)
