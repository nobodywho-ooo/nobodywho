---
title: Embeddings & RAG
description: Learn how to use embeddings and cross-encoders to build retrieval-augmented generation (RAG) systems with NobodyWho.
sidebar_position: 5
---

When you want your LLM to search through documents, understand semantic similarity, or build
retrieval-augmented generation (RAG) systems, you'll need embeddings and cross-encoders. In a
game, that's how an NPC can answer questions about your quest log, item descriptions, or lore
documents without any of it being in the model's training data.

## Understanding Embeddings

Embeddings convert text into vectors (lists of numbers) that capture semantic meaning. Texts with
similar meanings have similar vectors, even if they use different words.

For example, "Schedule a meeting for next Tuesday" and "Book an appointment next week" would have
very similar embeddings, despite using different words.

## The Encoder

The `NobodyWhoEncoder` object converts text into embedding vectors. You'll need a specialized
embedding model (different from chat models).

We recommend you first try
[bge-small-en-v1.5-q8_0.gguf](https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/main/bge-small-en-v1.5-q8_0.gguf).

```gdscript
var encoder = await NobodyWhoEncoder.create("./embedding-model.gguf", {})
var embedding: PackedFloat32Array = await encoder.encode("What is the weather like?")
print("Vector with %d dimensions" % embedding.size())
```

The resulting embedding is a `PackedFloat32Array` (typically 384 or 768 dimensions depending on
the model). As with chats, the model can also be shared — create a `NobodyWhoModel` once and pass
it to several encoders.

### Batch Encoding

Use `encode_batch` for multiple texts. It returns an array of vectors, in input order:

```gdscript
var texts = [
    "Paris is the capital of France.",
    "Berlin is the capital of Germany.",
]
var embeddings = await encoder.encode_batch(texts)
```

### Comparing Embeddings

Compare two embeddings with the static `cosine_similarity` — `1.0` means identical direction,
`0.0` unrelated:

```gdscript
var a = await encoder.encode("the cat sat on the mat")
var b = await encoder.encode("a feline rested on the rug")
print(NobodyWhoEncoder.cosine_similarity(a, b)) # high, e.g. 0.85
```

### Practical Example: Finding Relevant Documents

```gdscript
var documents = [
    "The healing potion restores 50 health points.",
    "The innkeeper sells rooms for 10 gold per night.",
    "Swords deal more damage than daggers, but swing slower.",
]
var doc_embeddings = await encoder.encode_batch(documents)

func find_most_relevant(query: String) -> int:
    var query_embedding = await encoder.encode(query)
    var best_idx := 0
    var best_score := -2.0
    for i in doc_embeddings.size():
        var score = NobodyWhoEncoder.cosine_similarity(query_embedding, doc_embeddings[i])
        if score > best_score:
            best_score = score
            best_idx = i
    return best_idx

print(documents[await find_most_relevant("where can I sleep?")])
# "The innkeeper sells rooms for 10 gold per night."
```

## The CrossEncoder for Better Ranking

### Why CrossEncoder Matters

The Encoder compares the query and each document *separately* — fast, but it can miss nuance
since the two never see each other. A **cross-encoder** reads the query and document *together*
and scores how relevant the document is to the query directly. That's more accurate, but slower —
so the standard pattern is: use the encoder to cheaply narrow hundreds of documents down to a
shortlist, then rerank that shortlist with the cross-encoder.

### Using CrossEncoder

We recommend
[bge-reranker-v2-m3-Q8_0.gguf](https://huggingface.co/gpustack/bge-reranker-v2-m3-GGUF/resolve/main/bge-reranker-v2-m3-Q8_0.gguf)
as a reranking model:

```gdscript
var reranker = await NobodyWhoCrossEncoder.create("./reranker-model.gguf", {})
var scores: PackedFloat32Array = await reranker.rank(
    "where can I sleep?",
    ["The healing potion restores 50 health points.", "The innkeeper sells rooms for 10 gold per night."],
)
print(scores) # e.g. [-3.2, 1.8] — higher is more relevant
```

### Automatic Sorting

`rank_and_sort` does the ranking and returns the documents sorted, most relevant first:

```gdscript
var ranked = await reranker.rank_and_sort("where can I sleep?", documents)
print(ranked[0]) # the most relevant document
```

## Building a RAG System

Put it together: embed your knowledge base once, retrieve the most relevant chunks for a query,
and inject them into the chat's system prompt:

```gdscript
var encoder = await NobodyWhoEncoder.create("./embedding-model.gguf", {})
var chat = await NobodyWhoChat.create("./model.gguf", {})
var knowledge = [
    "The bridge east of town was destroyed by trolls last spring.",
    "The ferry runs at dawn, but only when the river is calm.",
]
var knowledge_embeddings = await encoder.encode_batch(knowledge)

func answer_lore_question(question: String) -> String:
    var query_embedding = await encoder.encode(question)
    var best_idx := 0
    var best_score := -2.0
    for i in knowledge_embeddings.size():
        var score = NobodyWhoEncoder.cosine_similarity(query_embedding, knowledge_embeddings[i])
        if score > best_score:
            best_score = score
            best_idx = i
    await chat.set_system_prompt(
        "Answer questions using this lore: " + knowledge[best_idx]
    )
    return await chat.ask(question).completed()
```

## Recommended Models

### For Embeddings
- [bge-small-en-v1.5-q8_0.gguf](https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/main/bge-small-en-v1.5-q8_0.gguf) -
  Good balance of speed and quality (~25MB). Supports English text with 384-dimensional embeddings.

### For Cross-Encoding (Reranking)
- [bge-reranker-v2-m3-Q8_0.gguf](https://huggingface.co/gpustack/bge-reranker-v2-m3-GGUF/resolve/main/bge-reranker-v2-m3-Q8_0.gguf) -
  Multilingual support with excellent accuracy.

## Best Practices

**Precompute embeddings**: If you have a fixed knowledge base (your game's lore won't change at
runtime), generate embeddings once at load time — or even offline at build time — and reuse them.
Don't re-encode the same documents repeatedly.

**Use embeddings for filtering**: When working with large document collections (1000+ documents),
use embeddings to narrow down to the top 50-100 candidates, then use a cross-encoder to rerank
them.
