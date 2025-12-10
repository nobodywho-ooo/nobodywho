#!/usr/bin/env python3
"""
Simple cross-encoder demo showing how to rank documents by relevance to a query.
Usage: python crossencoder_demo.py /path/to/crossencoder/model.gguf
"""

import sys
import nobodywho


def main():
    if len(sys.argv) != 2:
        print("Usage: python crossencoder_demo.py /path/to/crossencoder/model.gguf")
        sys.exit(1)

    model_path = sys.argv[1]
    print(f"Loading cross-encoder model: {model_path}")

    # Create model and cross-encoder instance
    model = nobodywho.Model(model_path, use_gpu_if_available=True)
    crossencoder = nobodywho.CrossEncoder(model, n_ctx=4096)

    # Example query and documents
    query = "What is the capital of France?"
    documents = [
        "Paris is the capital and largest city of France.",
        "The Eiffel Tower is located in Paris, France.",
        "Berlin is the capital of Germany.",
        "London is the capital of the United Kingdom.",
        "France is a country in Western Europe.",
        "The French Revolution began in 1789.",
        "Tokyo is the capital of Japan.",
        "French cuisine is famous worldwide.",
    ]

    print(f"\nQuery: {query}")
    print(f"\nDocuments to rank ({len(documents)} total):")
    for i, doc in enumerate(documents):
        print(f"  {i + 1}. {doc}")

    # Rank documents
    print("\nRanking documents...")
    ranked_docs = crossencoder.rank_and_sort(query, documents)

    # Display results
    print("\nRanked results (most relevant first):")
    for i, (doc, score) in enumerate(ranked_docs):
        print(f"  {i + 1}. Score: {score:.3f} - {doc}")

    # Also demonstrate raw scoring
    print("\nRaw scores (in original order):")
    raw_scores = crossencoder.rank(query, documents)
    for i, (doc, score) in enumerate(zip(documents, raw_scores)):
        print(f"  {i + 1}. Score: {score:.3f} - {doc}")

    print("\nDone!")


if __name__ == "__main__":
    main()
