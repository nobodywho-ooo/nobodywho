# Shared GGML runtime

An internal crate shared by the standalone Supertonic and Gemma 4 POCs. It wraps the raw GGML API exposed by the direct bindings in `../ggml-sys`.

It provides:

- CPU and Apple Metal backends
- Tensor shapes and graph operations
- Backend weight and graph buffers
- GGUF loading, including audio.cpp logical-shape metadata
- Graph allocation, execution, synchronization, and transfers

Architecture-specific graphs, tokenization, audio processing, and generation remain in their respective POCs. This crate is experimental and is not a public NobodyWho API.
