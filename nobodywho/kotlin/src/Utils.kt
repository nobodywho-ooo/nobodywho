package com.nobodywho

import uniffi.nobodywho.cosineSimilarity as internalCosineSimilarity

/** Compute the cosine similarity between two vectors. */
fun cosineSimilarity(a: List<Float>, b: List<Float>): Float {
    return internalCosineSimilarity(a, b)
}
