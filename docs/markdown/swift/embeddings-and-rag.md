---
title: Embeddings & RAG
description: Using NobodyWho for embeddings and retrieval-augmented generation
sidebar_title: Embeddings & RAG
order: 5
---

NobodyWho provides `Encoder` and `CrossEncoder` classes for building retrieval-augmented generation (RAG) pipelines entirely on-device.

## Embeddings

An `Encoder` converts text into a numerical vector (embedding) that captures its semantic meaning. Texts with similar meanings will have similar embeddings.

```swift
import NobodyWho

let encoder = try await Encoder.fromPath(modelPath: "/path/to/embeddings.gguf")

let embedding1 = try await encoder.encode("The cat sat on the mat")
let embedding2 = try await encoder.encode("A feline rested on the rug")
let embedding3 = try await encoder.encode("The stock market crashed today")

let similar = cosineSimilarity(a: embedding1, b: embedding2)    // High similarity
let different = cosineSimilarity(a: embedding1, b: embedding3)  // Low similarity
```

You can also create an encoder from an already-loaded model:

```swift
let model = try await Model.load(modelPath: "/path/to/embeddings.gguf")
let encoder = Encoder(model: model)
```

## Cross-Encoder for reranking

A `CrossEncoder` takes a query and a list of documents and scores each document by its relevance to the query. Unlike embeddings (which are computed independently), a cross-encoder processes the query and document together, giving more accurate relevance scores.

```swift
let crossEncoder = try await CrossEncoder.fromPath(modelPath: "/path/to/reranker.gguf")

let query = "How do I reset my password?"
let documents = [
    "Click 'Forgot Password' on the login page.",
    "Our company was founded in 2020.",
    "Contact support for account recovery.",
    "The weather is sunny today.",
]

// Get raw similarity scores
let scores = try await crossEncoder.rank(query: query, documents: documents)

// Or get documents sorted by relevance (most relevant first)
let ranked = try await crossEncoder.rankAndSort(query: query, documents: documents)
for (document, score) in ranked {
    print("\(score): \(document)")
}
```

## Building a RAG pipeline

A typical RAG pipeline combines both tools:

1. **Index**: Use the `Encoder` to create embeddings for your document collection
2. **Retrieve**: When a user asks a question, embed the query and find the most similar documents using `cosineSimilarity`
3. **Rerank** (optional): Use the `CrossEncoder` to rerank the top candidates for better precision
4. **Generate**: Pass the relevant documents to a `Chat` as context in the system prompt

```swift
// 1. Embed your documents (do this once, store the results)
let encoder = try await Encoder.fromPath(modelPath: "/path/to/embeddings.gguf")
let docs = ["Document 1...", "Document 2...", "Document 3..."]
let docEmbeddings = try await docs.asyncMap { try await encoder.encode($0) }

// 2. Embed the query and find similar documents
let queryEmbedding = try await encoder.encode("What is the return policy?")
let similarities = docEmbeddings.map { cosineSimilarity(a: queryEmbedding, b: $0) }

// 3. Rerank the top results
let crossEncoder = try await CrossEncoder.fromPath(modelPath: "/path/to/reranker.gguf")
let topDocs = // ... select top N by similarity
let ranked = try await crossEncoder.rankAndSort(query: "What is the return policy?", documents: topDocs)

// 4. Generate a response with context
let context = ranked.prefix(3).map { $0.0 }.joined(separator: "\n\n")
let chat = try await Chat.fromPath(
    modelPath: "/path/to/model.gguf",
    systemPrompt: "Answer based on the following documents:\n\n\(context)"
)
let response = try await chat.ask("What is the return policy?").completed()
```
