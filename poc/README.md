# GGML prototypes

This workspace contains two standalone inference prototypes and their shared runtime:

- `ggml-supertonic`: Supertonic 3 text-to-speech on CPU and Metal
- `ggml-mimir`: DFM Mimir text generation on CPU and Metal
- `ggml-runtime`: shared GGML backend, tensor, graph, and model-loading code

Each prototype has its own download instructions and Makefile. The nested Cargo workspace keeps one lockfile and build directory without adding the prototypes to NobodyWho's production workspace.
