# Minimal Gemma 4 Metal runtime

This crate is a clean direct-Metal baseline for Gemma 4 E2B Q4_K_M. It does not use GGML for inference or graph execution. See [RESULTS.md](RESULTS.md) for benchmarks and portability notes.

It uses GGML only to parse the GGUF header and dequantize the two embedding gather buffers during loading. The runtime then:

- reads Q4_K, Q5_K, and Q6_K matrices directly from their original GGUF layouts
- preallocates F16 KV caches and F32 activation buffers
- encodes a complete token pass into one Metal command buffer
- runs custom GEMV, fused residual normalization, SIMD-cooperative decode attention, RoPE, GeGLU, cache, and argmax kernels
- supports the fixed 35-layer Gemma 4 E2B architecture and contexts up to 512 tokens

Run it with tokenized prompt IDs:

```bash
cargo run --release -p metal-gemma4 -- \
  --model-dir ../models/gemma-4-E2B-it \
  --prompt-tokens /tmp/prompt.json \
  --tokens 32 \
  --repetitions 5
```

The quantized runtime preserves all 32 greedy tokens for the default Portugal prompt. `THIRD_PARTY_NOTICES.md` documents the GGML-derived quantized GEMV arithmetic.
