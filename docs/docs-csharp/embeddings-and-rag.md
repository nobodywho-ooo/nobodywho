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

```csharp
using NobodyWho;

using var encoder = await Encoder.FromPathAsync("./embedding-model.gguf");
float[] embedding = await encoder.EncodeAsync("What is the weather like?");
Console.WriteLine($"Vector with {embedding.Length} dimensions");
```

To share an already loaded `Model`, use `new Encoder(model)` instead. Both take an optional `contextSize` (default 4096).

Use `EncodeBatchAsync` for multiple texts. It returns embeddings in input order:

```csharp
string[] texts =
[
    "Paris is the capital of France.",
    "Berlin is the capital of Germany.",
];
float[][] embeddings = await encoder.EncodeBatchAsync(texts);
```

### Comparing Embeddings

Measure how similar two pieces of text are with cosine similarity:

```csharp
using NobodyWho;

using var encoder = await Encoder.FromPathAsync("./embedding-model.gguf");

float[] query = await encoder.EncodeAsync("How do I reset my password?");
float[] doc1 = await encoder.EncodeAsync("You can reset your password in the account settings");
float[] doc2 = await encoder.EncodeAsync("The password requirements include 8 characters minimum");

float similarity1 = Encoder.CosineSimilarity(query, doc1);
float similarity2 = Encoder.CosineSimilarity(query, doc2);

Console.WriteLine($"Document 1 similarity: {similarity1:F3}");  // Higher score
Console.WriteLine($"Document 2 similarity: {similarity2:F3}");  // Lower score
```

Cosine similarity returns a value between -1 and 1, where 1 means identical meaning and -1 means opposite meaning.

### Finding relevant documents

```csharp
using NobodyWho;

using var encoder = await Encoder.FromPathAsync("./embedding-model.gguf");

string[] documents =
[
    "Python supports multiple programming paradigms including object-oriented and functional",
    "JavaScript is primarily used for web development and runs in browsers",
    "SQL is a domain-specific language for managing relational databases",
    "Git is a version control system for tracking changes in source code",
];

// Pre-compute document embeddings in a batch
float[][] docEmbeddings = await encoder.EncodeBatchAsync(documents);

float[] queryEmbedding = await encoder.EncodeAsync("What language should I use for database queries?");

// Find the most relevant document
var best = documents
    .Select((doc, i) => (Document: doc, Score: Encoder.CosineSimilarity(queryEmbedding, docEmbeddings[i])))
    .MaxBy(r => r.Score);

Console.WriteLine($"Most relevant: {best.Document}");
Console.WriteLine($"Similarity score: {best.Score:F3}");
```

## The CrossEncoder for Better Ranking

While embeddings work well for initial filtering, cross-encoders provide more accurate relevance scoring. They directly compare a query against documents rather than comparing vectors.

```csharp
using NobodyWho;

using var crossEncoder = await CrossEncoder.FromPathAsync("./reranker-model.gguf");

string query = "How do I install Python packages?";
string[] documents =
[
    "Someone previously asked about Python packages",
    "Use pip install package-name to install Python packages",
    "Python packages are not included in the standard library",
];

float[] scores = await crossEncoder.RankAsync(query, documents);
Console.WriteLine(string.Join(", ", scores));  // 0.23, 0.89, 0.45 - second doc scores highest
```

### Automatic Sorting

Use `RankAndSortAsync` to get documents sorted by relevance, as `(Document, Score)` tuples:

```csharp
var rankedDocs = await crossEncoder.RankAndSortAsync(query, documents);

foreach (var (doc, score) in rankedDocs)
{
    Console.WriteLine($"[{score:F3}] {doc}");
}
```

## Building a RAG System

Retrieval-Augmented Generation (RAG) combines document search with LLM generation. The LLM uses retrieved documents to ground its responses in your knowledge base.

```csharp
using System.ComponentModel;
using NobodyWho;

using var crossEncoder = await CrossEncoder.FromPathAsync("./reranker-model.gguf");

string[] knowledge =
[
    "Our company offers a 30-day return policy for all products",
    "Free shipping is available on orders over $50",
    "Customer support is available via email and phone",
    "We accept credit cards, PayPal, and bank transfers",
    "Order tracking is available through your account dashboard",
];

var searchTool = new Tool(
    "search_knowledge",
    "Search the knowledge base for relevant information",
    async ([Description("What to search for")] string query) =>
    {
        var ranked = await crossEncoder.RankAndSortAsync(query, knowledge);
        return string.Join("\n", ranked.Take(3).Select(r => r.Document));
    });

using var chat = await Chat.FromPathAsync(
    "./model.gguf",
    systemPrompt: "You are a customer service assistant. Use the search_knowledge tool to find relevant information before answering.",
    templateVariables: new Dictionary<string, bool> { ["enable_thinking"] = false },
    tools: [searchTool]);

string response = await chat.Ask("What is your return policy?").CompletedAsync();
Console.WriteLine(response);
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

**Choose an appropriate context size**: `contextSize` (default 4096) limits how long a single text can be. Raise it for longer documents:

```csharp
using var encoder = await Encoder.FromPathAsync("./embedding-model.gguf", contextSize: 8192);
using var crossEncoder = await CrossEncoder.FromPathAsync("./reranker-model.gguf", contextSize: 8192);
```
