package ai.nobodywho

import uniffi.nobodywho.cosineSimilarity as internalCosineSimilarity
import uniffi.nobodywho.getCachedModels as internalGetCachedModels

/** Compute the cosine similarity between two vectors. */
fun cosineSimilarity(a: List<Float>, b: List<Float>): Float {
    return internalCosineSimilarity(a, b)
}

/** Returns every cached `.gguf` model paired with its byte size. */
fun getCachedModels(): List<CachedModel> {
    return internalGetCachedModels()
}
