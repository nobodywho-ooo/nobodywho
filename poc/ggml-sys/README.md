# GGML bindings

Raw Rust bindings for [GGML](https://github.com/ggml-org/ggml). The crate builds the pinned GGML submodule with CPU support and adds Metal on macOS.

The bindings cover GGML and GGUF only. Higher-level tensor and graph wrappers live in `../ggml-runtime`.

Clone this repository with submodules before building:

```bash
git clone --recurse-submodules https://github.com/nobodywho-ooo/nobodywho.git
```
