---
title: Embeddings & RAG
description: Embeddings, similarity, and reranking in JavaScript
sidebar_position: 5
---

Beyond chat, NobodyWho can turn text into embedding vectors — the basis for semantic search and retrieval-augmented generation (RAG).

## Embedding text

Load an embedding model, create an `Encoder`, and call `encode()`:

```js
import createNobodyWhoModule from '@nobodywho/js';
const m = await createNobodyWhoModule();

const model = await m.Model.load({
  modelUrl: 'https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/main/bge-small-en-v1.5-q8_0.gguf',
  // Node: modelPath: '/path/to/embedding-model.gguf'
});
const encoder = new m.Encoder(model, 2048); // (model, contextSize)

const vec = await encoder.encode('the quick brown fox'); // Float32Array
```

## Similarity

Compare two embeddings with `cosineSimilarity` — higher means more similar:

```js
const a = await encoder.encode('cats and dogs');
const b = await encoder.encode('pets at home');
const score = m.cosineSimilarity(a, b);
```

A minimal semantic search: embed your documents up front, embed the query, and rank by similarity.

```js
const docs = ['Paris is in France.', 'The mitochondrion is the powerhouse of the cell.'];
const docVecs = await Promise.all(docs.map((d) => encoder.encode(d)));

const query = await encoder.encode('What is the capital of France?');
const ranked = docs
  .map((doc, i) => ({ doc, score: m.cosineSimilarity(query, docVecs[i]) }))
  .sort((x, y) => y.score - x.score);
// ranked[0].doc -> 'Paris is in France.'
```

## Reranking with a cross-encoder

For higher-quality ranking, a `CrossEncoder` scores each (query, document) pair directly instead of comparing independent embeddings:

```js
const reranker = await m.Model.load({
  modelUrl: 'https://huggingface.co/gpustack/bge-reranker-v2-m3-GGUF/resolve/main/bge-reranker-v2-m3-Q8_0.gguf',
});
const cross = new m.CrossEncoder(reranker, 4096);

const ranked = await cross.rankAndSort('What is the capital of France?', docs);
for (const [doc, score] of ranked) {
  console.log(score.toFixed(3), doc); // best first
}
```

Use `rank(query, docs)` instead if you want the raw scores in the original order.
