# GGML prototypes

This workspace contains two standalone inference prototypes and their shared GGML crates:

- `ggml-supertonic`: Supertonic 3 text-to-speech on CPU and Metal
- `ggml-gemma4`: Gemma 4 E2B prompt and generation benchmarks on Metal
- `ggml-runtime`: shared GGML backend, tensor, graph, and model-loading code
- `ggml-sys`: direct bindings to the pinned GGML source

Initialize the pinned GGML source before building:

```bash
git submodule update --init poc/ggml-sys/ggml
```

Each prototype has its own download instructions and Makefile. The nested Cargo workspace keeps one lockfile and build directory without adding the prototypes to NobodyWho's production workspace.
