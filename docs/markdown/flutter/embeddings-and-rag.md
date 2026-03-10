---
title: Embeddings & RAG
description: Learn how to use embeddings and cross-encoders to build retrieval-augmented generation (RAG) systems with NobodyWho.
sidebar_title: Embeddings & RAG
order: 5
---

When you want your LLM to search through documents, understand semantic similarity, or build retrieval-augmented generation (RAG) systems, you'll need embeddings and cross-encoders.

## Understanding Embeddings

Embeddings convert text into vectors (lists of numbers) that capture semantic meaning. Texts with similar meanings have similar vectors, even if they use different words.

For example, "Schedule a meeting for next Tuesday" and "Book an appointment next week" would have very similar embeddings, despite using different words.

## The Encoder

The `Encoder` object converts text into embedding vectors. You'll need a specialized embedding model (different from chat models).

We recommend you first try [bge-small-en-v1.5-q8_0.gguf](https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/main/bge-small-en-v1.5-q8_0.gguf).

```dart
import 'dart:typed_data';

final encoder = await nobodywho.Encoder.fromPath(modelPath: './embedding-model.gguf');
final embedding = await encoder.encode(text: "What is the weather like?");
print("Vector with ${embedding.length} dimensions");
```

The resulting embedding is a `Float32List` (typically 384 or 768 dimensions depending on the model).

### Comparing Embeddings

To measure how similar two pieces of text are, compare their embeddings using cosine similarity:

```dart
import 'dart:typed_data';
import 'package:nobodywho/nobodywho.dart' as nobodywho;

final encoder = await nobodywho.Encoder.fromPath(modelPath: './embedding-model.gguf');

final query = await encoder.encode(text: "How do I reset my password?");
final doc1 = await encoder.encode(text: "You can reset your password in the account settings");
final doc2 = await encoder.encode(text: "The password requirements include 8 characters minimum");

final similarity1 = nobodywho.cosineSimilarity(
  a: query.toList(),
  b: doc1.toList()
);
final similarity2 = nobodywho.cosineSimilarity(
  a: query.toList(),
  b: doc2.toList()
);

print("Document 1 similarity: ${similarity1.toStringAsFixed(3)}");  // Higher score
print("Document 2 similarity: ${similarity2.toStringAsFixed(3)}");  // Lower score
```

Cosine similarity returns a value between -1 and 1, where 1 means identical meaning and -1 means opposite meaning.

### Practical Example: Finding Relevant Documents

```dart
import 'dart:typed_data';
import 'package:nobodywho/nobodywho.dart' as nobodywho;

final encoder = await nobodywho.Encoder.fromPath(modelPath: './embedding-model.gguf');

// Your knowledge base
final documents = [
  "Python supports multiple programming paradigms including object-oriented and functional",
  "JavaScript is primarily used for web development and runs in browsers",
  "SQL is a domain-specific language for managing relational databases",
  "Git is a version control system for tracking changes in source code"
];

// Pre-compute document embeddings
final docEmbeddings = <Float32List>[];
for (final doc in documents) {
  docEmbeddings.add(await encoder.encode(text: doc));
}

// Search query
final query = "What language should I use for database queries?";
final queryEmbedding = await encoder.encode(text: query);

// Find the most relevant document
double maxSimilarity = -1;
int bestIdx = 0;
for (int i = 0; i < docEmbeddings.length; i++) {
  final similarity = nobodywho.cosineSimilarity(
    a: queryEmbedding.toList(),
    b: docEmbeddings[i].toList()
  );
  if (similarity > maxSimilarity) {
    maxSimilarity = similarity;
    bestIdx = i;
  }
}

print("Most relevant: ${documents[bestIdx]}");
print("Similarity score: ${maxSimilarity.toStringAsFixed(3)}");
```

## The CrossEncoder for Better Ranking

While embeddings work well for initial filtering, cross-encoders provide more accurate relevance scoring. They directly compare a query against documents to determine how well the document answers the query.

The key difference is that embeddings compare vector similarity, while cross-encoders understand the relationship between query and document, at a potentially larger computation cost.

### Why CrossEncoder Matters

Consider this example:

```
Query: "What are the office hours for customer support?"
Documents: [
    "Customer asked: What are the office hours for customer support?",
    "Support team responds: Our customer support is available Monday-Friday 9am-5pm EST",
    "Note: Weekend support is not available at this time"
]
```

Using embeddings alone, the first document scores highest (most similar to the query) even though it provides no useful information. A cross-encoder correctly identifies that the second document actually answers the question.

### Using CrossEncoder

```dart
import 'package:nobodywho/nobodywho.dart' as nobodywho;

// Download a reranking model like bge-reranker-v2-m3-Q8_0.gguf
final crossencoder = await nobodywho.CrossEncoder.fromPath(modelPath: './reranker-model.gguf');

final query = "How do I install Python packages?";
final documents = [
  "Someone previously asked about Python packages",
  "Use pip install package-name to install Python packages",
  "Python packages are not included in the standard library"
];

// Get relevance scores for each document
final scores = await crossencoder.rank(query: query, documents: documents);
print(scores);  // [0.23, 0.89, 0.45] - second doc scores highest
```

### Automatic Sorting

For convenience, use `rankAndSort` to get documents sorted by relevance:

```{.dart continuation}
// Returns list of (document, score) tuples, sorted by score
final rankedDocs = await crossencoder.rankAndSort(query: query, documents: documents);

for (final (doc, score) in rankedDocs) {
  print("[${score.toStringAsFixed(3)}] $doc");
}
```

This returns documents ordered from most to least relevant.

## Building a RAG System

Retrieval-Augmented Generation (RAG) combines document search with LLM generation. The LLM uses retrieved documents to ground its responses in your knowledge base.

Here's a complete example building a customer service assistant with access to company policies:

```dart
import 'package:nobodywho/nobodywho.dart' as nobodywho;

Future<void> main() async {
  // Initialize the cross-encoder for document ranking
  final crossencoder = await nobodywho.CrossEncoder.fromPath(modelPath: './reranker-model.gguf');

  // Your knowledge base
  final knowledge = [
    "Our company offers a 30-day return policy for all products",
    "Free shipping is available on orders over \$50",
    "Customer support is available via email and phone",
    "We accept credit cards, PayPal, and bank transfers",
    "Order tracking is available through your account dashboard"
  ];

  // Create a tool that searches the knowledge base
  final searchKnowledgeTool = nobodywho.Tool(
    function: ({required String query}) async {
      // Rank all documents by relevance to the query
      final ranked = await crossencoder.rankAndSort(query: query, documents: knowledge);
      
      // Return top 3 most relevant documents
      final topDocs = ranked.take(3).map((e) => e.$1).toList();
      return topDocs.join("\n");
    },
    name: "search_knowledge",
    description: "Search the knowledge base for relevant information"
  );

  // Create a chat with access to the knowledge base
  final chat = await nobodywho.Chat.fromPath(
    modelPath: './model.gguf',
    systemPrompt: "You are a customer service assistant. Use the search_knowledge tool to find relevant information from our policies before answering customer questions.",
    tools: [searchKnowledgeTool]
  );

  // The chat will automatically search the knowledge base when needed
  final response = await chat.ask("What is your return policy?").completed();
  print(response);
}
```

The LLM will call the `search_knowledge` tool, receive the most relevant documents, and use them to generate an accurate answer.

## Async Operations

In Flutter/Dart, all operations are asynchronous by default. There are no separate `EncoderAsync` or `CrossEncoderAsync` classes - the regular `Encoder` and `CrossEncoder` classes use async/await patterns:

```dart
import 'dart:typed_data';
import 'package:nobodywho/nobodywho.dart' as nobodywho;

Future<void> main() async {
  final encoder = await nobodywho.Encoder.fromPath(modelPath: './embedding-model.gguf');
  
  final crossencoder = await nobodywho.CrossEncoder.fromPath(modelPath: './reranker-model.gguf');
  
  // Generate embeddings asynchronously
  final embedding = await encoder.encode(text: "What is the weather?");
  
  // Rank documents asynchronously
  final query = "What is our refund policy?";
  final docs = [
    "Refunds processed within 5-7 business days",
    "No refunds on sale items",
    "Contact support to initiate refund"
  ];
  final ranked = await crossencoder.rankAndSort(query: query, documents: docs);
  
  for (final (doc, score) in ranked) {
    print("[${score.toStringAsFixed(3)}] $doc");
  }
}
```


## Recommended Models

### For Embeddings
- [bge-small-en-v1.5-q8_0.gguf](https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/main/bge-small-en-v1.5-q8_0.gguf) - Good balance of speed and quality (~25MB)
- Supports English text with 384-dimensional embeddings

### For Cross-Encoding (Reranking)
- [bge-reranker-v2-m3-Q8_0.gguf](https://huggingface.co/gpustack/bge-reranker-v2-m3-GGUF/resolve/main/bge-reranker-v2-m3-Q8_0.gguf) - Multilingual support with excellent accuracy

## Best Practices

**Precompute embeddings**: If you have a fixed knowledge base, generate embeddings once and reuse them. Don't re-encode the same documents repeatedly.

**Use embeddings for filtering**: When working with large document collections (1000+ documents), use embeddings to narrow down to the top 50-100 candidates, then use a cross-encoder to rerank them.

**Limit cross-encoder inputs**: Cross-encoders are more expensive than embeddings. Don't pass thousands of documents to `rank()` - filter first with embeddings.

**Choose appropriate context size**: The `nCtx` parameter (default 4096) should match your model's recommended context size. Check the model documentation.

```dart
// For longer documents, increase context size
final encoder = await nobodywho.Encoder.fromPath(modelPath: './embedding-model.gguf');

final crossencoder = await nobodywho.CrossEncoder.fromPath(modelPath: './reranker-model.gguf');
```

## Complete RAG Example

Here's a full example showing a two-stage retrieval system:

```dart
import 'dart:typed_data';
import 'package:nobodywho/nobodywho.dart' as nobodywho;

Future<void> main() async {
  // Initialize models
  final encoder = await nobodywho.Encoder.fromPath(modelPath: './embedding-model.gguf');
  
  final crossencoder = await nobodywho.CrossEncoder.fromPath((modelPath: './reranker-model.gguf');)

  // Large knowledge base
  final knowledgeBase = [
    "Python 3.11 introduced performance improvements through faster CPython",
    "The Django framework is used for building web applications",
    "NumPy provides support for large multi-dimensional arrays",
    "Pandas is the standard library for data manipulation and analysis",
    // ... 100+ more documents
  ];

  // Precompute embeddings for all documents
  final docEmbeddings = <Float32List>[];
  for (final doc in knowledgeBase) {
    docEmbeddings.add(await encoder.encode(text: doc));
  }

  Future<String> search({required String query}) async {
    // Stage 1: Fast filtering with embeddings
    final queryEmbedding = await encoder.encode(text: query);
    final similarities = <(String, double)>[];
    for (int i = 0; i < knowledgeBase.length; i++) {
      final similarity = nobodywho.cosineSimilarity(
        a: queryEmbedding.toList(),
        b: docEmbeddings[i].toList()
      );
      similarities.add((knowledgeBase[i], similarity));
    }
    // Get top 20 candidates
    similarities.sort((a, b) => b.$2.compareTo(a.$2));
    final candidateDocs = similarities.take(20).map((e) => e.$1).toList();
    
    // Stage 2: Precise ranking with cross-encoder
    final ranked = await crossencoder.rankAndSort(query: query, documents: candidateDocs);
    
    // Return top 3 most relevant
    final topResults = ranked.take(3).map((e) => e.$1).toList();
    return topResults.join("\n---\n");
  }

  final searchTool = nobodywho.Tool(
    function: search,
    name: "search",
    description: "Search the knowledge base for information relevant to the query"
  );

  // Create RAG-enabled chat
  final chat = await nobodywho.Chat.fromPath(
    modelPath: './model.gguf',
    systemPrompt: "You are a technical documentation assistant. Always use the search tool to find relevant information before answering programming questions.",
    tools: [searchTool]
  );

  // The chat automatically searches and uses retrieved documents
  final response = await chat.ask("What Python libraries are best for data analysis?").completed();
  print(response);
}
```

This two-stage approach combines the speed of embeddings with the accuracy of cross-encoders, making it efficient even for large knowledge bases.

