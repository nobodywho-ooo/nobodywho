#!/usr/bin/env python3
"""
Simple embeddings demo showing how to generate embeddings and compute similarity.
Usage: python embeddings_demo.py /path/to/embeddings/model.gguf
"""

import sys
import nobodywho


def main():
    if len(sys.argv) != 2:
        print("Usage: python embeddings_demo.py /path/to/embeddings/model.gguf")
        sys.exit(1)

    model_path = sys.argv[1]
    print(f"Loading embeddings model: {model_path}")

    # Create model and encoder instance
    model = nobodywho.Model(model_path, use_gpu_if_available=True)
    encoder = nobodywho.Encoder(model, n_ctx=2048)

    # Example texts
    texts = [
        "The quick brown fox jumps over the lazy dog.",
        "A fast brown fox leaps over the sleepy dog.",
        "The weather is sunny today.",
        "Machine learning is a subset of artificial intelligence.",
        "Deep learning uses neural networks with many layers.",
    ]

    print("\nGenerating embeddings...")
    text_embeddings = []
    for i, text in enumerate(texts):
        print(f"  {i + 1}. {text}")
        embedding = encoder.encode(text)
        text_embeddings.append((text, embedding))

    print(f"\nEmbedding dimension: {len(text_embeddings[0][1])}")

    # Compute pairwise similarities
    print("\nPairwise cosine similarities:")
    for i in range(len(texts)):
        for j in range(i + 1, len(texts)):
            similarity = nobodywho.cosine_similarity(
                text_embeddings[i][1], text_embeddings[j][1]
            )
            print(f"  Text {i + 1} vs Text {j + 1}: {similarity:.3f}")

    print("\nDone!")


if __name__ == "__main__":
    main()
