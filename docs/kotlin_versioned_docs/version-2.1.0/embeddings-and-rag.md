---
title: Embeddings & RAG
description: Learn how to use embeddings and cross-encoders to build retrieval-augmented generation (RAG) systems with NobodyWho.
sidebar_position: 5
---

When you want your LLM to search through documents, understand semantic similarity, or build retrieval-augmented generation (RAG) systems, you'll need embeddings and cross-encoders.

## Understanding Embeddings

Embeddings convert text into vectors (lists of numbers) that capture semantic meaning. Texts with similar meanings have similar vectors, even if they use different words.

## The Encoder

The `Encoder` converts text into embedding vectors. You'll need a specialized embedding model (different from chat models).

We recommend [bge-small-en-v1.5-q8_0.gguf](https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/main/bge-small-en-v1.5-q8_0.gguf).

```kotlin
import ai.nobodywho.Encoder

val encoder = Encoder.fromPath(modelPath = "./embedding-model.gguf")
val embedding = encoder.encode("What is the weather like?")
println("Vector with ${embedding.size} dimensions")
```

### Comparing Embeddings

Measure how similar two pieces of text are with cosine similarity:

```kotlin
import ai.nobodywho.Encoder
import ai.nobodywho.cosineSimilarity

val encoder = Encoder.fromPath(modelPath = "./embedding-model.gguf")

val query = encoder.encode("How do I reset my password?")
val doc1 = encoder.encode("You can reset your password in the account settings")
val doc2 = encoder.encode("The password requirements include 8 characters minimum")

val similarity1 = cosineSimilarity(query, doc1)
val similarity2 = cosineSimilarity(query, doc2)

println("Document 1 similarity: ${"%.3f".format(similarity1)}")  // Higher score
println("Document 2 similarity: ${"%.3f".format(similarity2)}")  // Lower score
```

## The CrossEncoder for Better Ranking

While embeddings work well for initial filtering, cross-encoders provide more accurate relevance scoring. They directly compare a query against documents rather than comparing vectors.

```kotlin
import ai.nobodywho.CrossEncoder

val crossEncoder = CrossEncoder.fromPath(modelPath = "./reranker-model.gguf")

val query = "How do I install Python packages?"
val documents = listOf(
    "Someone previously asked about Python packages",
    "Use pip install package-name to install Python packages",
    "Python packages are not included in the standard library"
)

val scores = crossEncoder.rank(query, documents)
println(scores)  // [0.23, 0.89, 0.45] - second doc scores highest
```

### Automatic Sorting

Use `rankAndSort` to get documents sorted by relevance:

```kotlin
val rankedDocs = crossEncoder.rankAndSort(query, documents)

for ((doc, score) in rankedDocs) {
    println("[${"%.3f".format(score)}] $doc")
}
```

## Building a RAG System

Retrieval-Augmented Generation (RAG) combines document search with LLM generation. The LLM uses retrieved documents to ground its responses in your knowledge base.

```kotlin
import ai.nobodywho.*

suspend fun main() {
    val crossEncoder = CrossEncoder.fromPath(modelPath = "./reranker-model.gguf")

    val knowledge = listOf(
        "Our company offers a 30-day return policy for all products",
        "Free shipping is available on orders over $50",
        "Customer support is available via email and phone",
        "We accept credit cards, PayPal, and bank transfers",
        "Order tracking is available through your account dashboard"
    )

    fun searchKnowledge(query: String): String {
        val ranked = runBlocking { crossEncoder.rankAndSort(query, knowledge) }
        return ranked.take(3).joinToString("\n") { it.first }
    }

    val searchTool = Tool(
        name = "search_knowledge",
        description = "Search the knowledge base for relevant information",
        function = ::searchKnowledge
    )

    val chat = Chat.fromPath(
        modelPath = "./model.gguf",
        systemPrompt = "You are a customer service assistant. Use the search_knowledge tool to find relevant information before answering.",
        templateVariables = mapOf("enable_thinking" to false),
        tools = listOf(searchTool)
    )

    val response = chat.ask("What is your return policy?").completed()
    println(response)
}
```

## Recommended Models

### For Embeddings
- [bge-small-en-v1.5-q8_0.gguf](https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/main/bge-small-en-v1.5-q8_0.gguf) - Good balance of speed and quality (~25MB)

### For Cross-Encoding (Reranking)
- [bge-reranker-v2-m3-Q8_0.gguf](https://huggingface.co/gpustack/bge-reranker-v2-m3-GGUF/resolve/main/bge-reranker-v2-m3-Q8_0.gguf) - Multilingual support with excellent accuracy

## Best Practices

**Precompute embeddings**: If you have a fixed knowledge base, generate embeddings once and reuse them.

**Use embeddings for filtering**: For large collections (1000+ documents), use embeddings to narrow down to the top 50-100 candidates, then use a cross-encoder to rerank.

**Limit cross-encoder inputs**: Cross-encoders are more expensive than embeddings. Filter first with embeddings, then rerank.
