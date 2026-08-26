# Shared GGML runtime

An internal crate shared by the standalone Supertonic and Mimir POCs. It wraps the raw GGML API from the same pinned `llama-cpp-sys-2` revision used by NobodyWho.

It provides:

- CPU and Apple Metal backends
- Tensor shapes and graph operations
- Backend weight and graph buffers
- GGUF loading, including audio.cpp logical-shape metadata
- Safetensors loading with BF16, F16, F32, and I32 tensors
- Graph allocation, execution, synchronization, and transfers

Architecture-specific graphs, tokenization, audio processing, and generation remain in their respective POCs. This crate is experimental and is not a public NobodyWho API.
