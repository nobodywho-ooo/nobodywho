---
title: Embeddings & RAG
description: NobodyWho is a lightweight, open-source AI engine for local LLM inference. Simple, privacy oriented with no infrastructure needed.
sidebar_title: Embeddings & RAG
order: 4
---

When you want your LLM to search through documents, understand semantic similarity, or build retrieval-augmented generation (RAG) systems, you'll need embeddings and cross-encoders.

## Understanding Embeddings

Embeddings convert text into vectors (lists of numbers) that capture semantic meaning. Texts with similar meanings have similar vectors, even if they use different words.

For example, "Hand me the red potion" and "Give me the scarlet flask" would have very similar embeddings, despite sharing no common words.

## The Encoder

The `Encoder` object converts text into embedding vectors. You'll need a specialized embedding model (different from chat models).

We recommend you first try [bge-small-en-v1.5-q8_0.gguf](https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/main/bge-small-en-v1.5-q8_0.gguf).

```python
from nobodywho import Encoder

encoder = Encoder('./embedding-model.gguf')
embedding = encoder.encode("What is the weather like?")
print(f"Vector with {len(embedding)} dimensions")
```

The resulting embedding is a list of floats (typically 384 or 768 dimensions depending on the model).

### Comparing Embeddings

To measure how similar two pieces of text are, compare their embeddings using cosine similarity:

```python
from nobodywho import Encoder, cosine_similarity

encoder = Encoder('./embedding-model.gguf')

query = encoder.encode("Where do I find crystals?")
doc1 = encoder.encode("Crystals are found in the Northern Mountains")
doc2 = encoder.encode("The Southern Desert has no crystals")

similarity1 = cosine_similarity(query, doc1)
similarity2 = cosine_similarity(query, doc2)

print(f"Document 1 similarity: {similarity1:.3f}")  # Higher score
print(f"Document 2 similarity: {similarity2:.3f}")  # Lower score
```

Cosine similarity returns a value between -1 and 1, where 1 means identical meaning and -1 means opposite meaning.

### Practical Example: Finding Relevant Documents

```python
from nobodywho import Encoder, cosine_similarity

encoder = Encoder('./embedding-model.gguf')

# Your knowledge base
documents = [
    "The blacksmith sells weapons and armor",
    "The inn provides food and lodging",
    "The library contains ancient scrolls and books",
    "The market sells fruits and vegetables"
]

# Pre-compute document embeddings
doc_embeddings = [encoder.encode(doc) for doc in documents]

# Search query
query = "Where can I buy a sword?"
query_embedding = encoder.encode(query)

# Find the most relevant document
similarities = [cosine_similarity(query_embedding, doc_emb) for doc_emb in doc_embeddings]
best_idx = similarities.index(max(similarities))

print(f"Most relevant: {documents[best_idx]}")
print(f"Similarity score: {similarities[best_idx]:.3f}")
```

## The CrossEncoder for Better Ranking

While embeddings work well for initial filtering, cross-encoders provide more accurate relevance scoring. They directly compare a query against documents to determine how well the document answers the query.

The key difference: embeddings compare vector similarity, while cross-encoders understand the relationship between query and document.

### Why CrossEncoder Matters

Consider this example:

```
Query: "Where do I find crystals for my sword upgrade?"
Documents: [
    "You asked the blacksmith: Where do I find crystals for my sword upgrade?",
    "The blacksmith said: Magic crystals are found in the Northern Mountains",
    "You heard in the tavern: Magic crystals are not found in the Southern Desert"
]
```

Using embeddings alone, the first document scores highest (most similar to the query) even though it provides no useful information. A cross-encoder correctly identifies that the second document actually answers the question.

### Using CrossEncoder

```python
from nobodywho import CrossEncoder

# Download a reranking model like bge-reranker-v2-m3-Q8_0.gguf
crossencoder = CrossEncoder('./reranker-model.gguf')

query = "Where can I find magic crystals?"
documents = [
    "You asked about crystals yesterday",
    "Magic crystals are found in the Northern Mountains",
    "The Southern Desert has no crystals"
]

# Get relevance scores for each document
scores = crossencoder.rank(query, documents)
print(scores)  # [0.23, 0.89, 0.45] - second doc scores highest
```

### Automatic Sorting

For convenience, use `rank_and_sort` to get documents sorted by relevance:

```{.python continuation}
# Returns list of (document, score) tuples, sorted by score
ranked_docs = crossencoder.rank_and_sort(query, documents)

for doc, score in ranked_docs:
    print(f"[{score:.3f}] {doc}")
```

This returns documents ordered from most to least relevant.

## Building a RAG System

Retrieval-Augmented Generation (RAG) combines document search with LLM generation. The LLM uses retrieved documents to ground its responses in your knowledge base.

Here's a complete example building a smart assistant with access to game lore:

```python
from nobodywho import Chat, CrossEncoder

# Initialize the cross-encoder for document ranking
crossencoder = CrossEncoder('./reranker-model.gguf')

# Your knowledge base
knowledge = [
    "The Crystal Mines contain valuable gems but are dangerous",
    "Blacksmith Gareth pays double for rare materials",
    "The old mine has been abandoned for years",
    "Strange sounds echo from deep inside the mine at night",
    "Magic crystals form naturally in the southern mines"
]

# Create a tool that searches the knowledge base
from nobodywho import tool

@tool(description="Search the knowledge base for relevant information")
def search_knowledge(query: str) -> str:
    # Rank all documents by relevance to the query
    ranked = crossencoder.rank_and_sort(query, knowledge)
    
    # Return top 3 most relevant documents
    top_docs = [doc for doc, score in ranked[:3]]
    return "\n".join(top_docs)

# Create a chat with access to the knowledge base
chat = Chat(
    './model.gguf',
    system_prompt="You are a helpful assistant. Use the search_knowledge tool to find relevant information before answering questions.",
    tools=[search_knowledge]
)

# The chat will automatically search the knowledge base when needed
response = chat.ask("Where can I find crystals?").completed()
print(response)
```

The LLM will call the `search_knowledge` tool, receive the most relevant documents, and use them to generate an accurate answer.

## Async API

For non-blocking operations, use `EncoderAsync` and `CrossEncoderAsync`:

```python
import asyncio
from nobodywho import EncoderAsync, CrossEncoderAsync

async def main():
    encoder = EncoderAsync('./embedding-model.gguf')
    crossencoder = CrossEncoderAsync('./reranker-model.gguf')
    
    # Generate embeddings asynchronously
    embedding = await encoder.encode("What is the weather?")
    
    # Rank documents asynchronously
    query = "Where are the crystals?"
    docs = ["In the mountains", "In the desert", "In the caves"]
    ranked = await crossencoder.rank_and_sort(query, docs)
    
    for doc, score in ranked:
        print(f"[{score:.3f}] {doc}")

asyncio.run(main())
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

**Choose appropriate context size**: The `n_ctx` parameter (default 2048) should match your model's recommended context size. Check the model documentation.

```python
# For longer documents, increase context size
encoder = Encoder('./embedding-model.gguf', n_ctx=4096)
crossencoder = CrossEncoder('./reranker-model.gguf', n_ctx=4096)
```

## Complete RAG Example

Here's a full example showing a two-stage retrieval system:

```python
from nobodywho import Chat, Encoder, CrossEncoder, cosine_similarity, tool

# Initialize models
encoder = Encoder('./embedding-model.gguf')
crossencoder = CrossEncoder('./reranker-model.gguf')

# Large knowledge base
knowledge_base = [
    "The ancient library contains scrolls about dragon lore",
    "Blacksmith Gareth forges legendary weapons",
    # ... 100+ more documents
]

# Precompute embeddings for all documents
doc_embeddings = [encoder.encode(doc) for doc in knowledge_base]

@tool(description="Search the knowledge base for information relevant to the query")
def search(query: str) -> str:
    # Stage 1: Fast filtering with embeddings
    query_embedding = encoder.encode(query)
    similarities = [
        (doc, cosine_similarity(query_embedding, doc_emb))
        for doc, doc_emb in zip(knowledge_base, doc_embeddings)
    ]
    # Get top 20 candidates
    candidates = sorted(similarities, key=lambda x: x[1], reverse=True)[:20]
    candidate_docs = [doc for doc, _ in candidates]
    
    # Stage 2: Precise ranking with cross-encoder
    ranked = crossencoder.rank_and_sort(query, candidate_docs)
    
    # Return top 3 most relevant
    top_results = [doc for doc, score in ranked[:3]]
    return "\n---\n".join(top_results)

# Create RAG-enabled chat
chat = Chat(
    './model.gguf',
    system_prompt="You are a knowledgeable assistant. Always use the search tool to find relevant information before answering.",
    tools=[search]
)

# The chat automatically searches and uses retrieved documents
response = chat.ask("Tell me about legendary weapons").completed()
print(response)
```

This two-stage approach combines the speed of embeddings with the accuracy of cross-encoders, making it efficient even for large knowledge bases.
