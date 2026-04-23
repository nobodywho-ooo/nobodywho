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

```typescript
import { Encoder } from "react-native-nobodywho";

const encoder = await Encoder.fromPath({
  modelPath: "/path/to/embedding-model.gguf",
});
const embedding = await encoder.encode("What is the weather like?");
console.log(`Vector with ${embedding.length} dimensions`);
```

The resulting embedding is an array of numbers (typically 384 or 768 dimensions depending on the model).

### Comparing Embeddings

To measure how similar two pieces of text are, compare their embeddings using cosine similarity:

```typescript
import { Encoder, cosineSimilarity } from "react-native-nobodywho";

const encoder = await Encoder.fromPath({
  modelPath: "/path/to/embedding-model.gguf",
});

const query = await encoder.encode("How do I reset my password?");
const doc1 = await encoder.encode(
  "You can reset your password in the account settings",
);
const doc2 = await encoder.encode(
  "The password requirements include 8 characters minimum",
);

const similarity1 = cosineSimilarity(query, doc1);
const similarity2 = cosineSimilarity(query, doc2);

console.log(`Document 1 similarity: ${similarity1.toFixed(3)}`); // Higher score
console.log(`Document 2 similarity: ${similarity2.toFixed(3)}`); // Lower score
```

Cosine similarity returns a value between -1 and 1, where 1 means identical meaning and -1 means opposite meaning.

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

```typescript
import { CrossEncoder } from "react-native-nobodywho";

// Download a reranking model like bge-reranker-v2-m3-Q8_0.gguf
const crossencoder = await CrossEncoder.fromPath({
  modelPath: "/path/to/reranker-model.gguf",
});

const query = "How do I install Python packages?";
const documents = [
  "Someone previously asked about Python packages",
  "Use pip install package-name to install Python packages",
  "Python packages are not included in the standard library",
];

// Get relevance scores for each document
const scores = await crossencoder.rank(query, documents);
console.log(scores); // [0.23, 0.89, 0.45] - second doc scores highest
```

### Automatic Sorting

For convenience, use `rankAndSort` to get documents sorted by relevance:

```typescript
// Returns list of [document, score] pairs, sorted by score
const rankedDocs = await crossencoder.rankAndSort(query, documents);

for (const [doc, score] of rankedDocs) {
  console.log(`[${score.toFixed(3)}] ${doc}`);
}
```

This returns documents ordered from most to least relevant.

## Building a RAG System

Retrieval-Augmented Generation (RAG) combines document search with LLM generation. The LLM uses retrieved documents to ground its responses in your knowledge base.

Here's a complete example building a customer service assistant with access to company policies:

```typescript
import { Chat, Tool, CrossEncoder } from "react-native-nobodywho";

// Initialize the cross-encoder for document ranking
const crossencoder = await CrossEncoder.fromPath({
  modelPath: "/path/to/reranker-model.gguf",
});

// Your knowledge base
const knowledge = [
  "Our company offers a 30-day return policy for all products",
  "Free shipping is available on orders over $50",
  "Customer support is available via email and phone",
  "We accept credit cards, PayPal, and bank transfers",
  "Order tracking is available through your account dashboard",
];

// Create a tool that searches the knowledge base
const searchKnowledgeTool = new Tool({
  name: "search_knowledge",
  description: "Search the knowledge base for relevant information",
  parameters: [
    { name: "query", type: "string", description: "The search query" },
  ],
  call: async (query: string) => {
    const ranked = await crossencoder.rankAndSort(query, knowledge);
    const topDocs = ranked
      .slice(0, 3)
      .map(([doc]) => doc);
    return topDocs.join("\n");
  },
});

// Create a chat with access to the knowledge base
const chat = await Chat.fromPath({
  modelPath: "/path/to/model.gguf",
  systemPrompt:
    "You are a customer service assistant. Use the search_knowledge tool to find relevant information from our policies before answering customer questions.",
  tools: [searchKnowledgeTool],
});

// The chat will automatically search the knowledge base when needed
const response = await chat.ask("What is your return policy?").completed();
console.log(response);
```

The LLM will call the `search_knowledge` tool, receive the most relevant documents, and use them to generate an accurate answer.

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
