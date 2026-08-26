# GGML Supertonic POC

A standalone Rust inference engine for Supertonic 3 using the raw GGML API exposed by `llama-cpp-sys-2`. It supports CPU and Apple Metal and does not use ONNX Runtime.

The POC implements the complete batch-one synthesis path:

1. Unicode text encoding
2. Duration prediction
3. Text encoding
4. Iterative vector estimation
5. Vocoding
6. Mono 44.1 kHz WAV output

It loads the original-precision GGUF package published for audio.cpp. Model files are downloaded locally and ignored by Git.

## Run

```sh
cd poc/ggml-supertonic
make model
make run
make run-metal
```

Override the defaults through Make:

```sh
make run-metal TEXT='Bonjour depuis NobodyWho.' OUTPUT=bonjour.wav
```

Or use Cargo directly:

```sh
cargo run --release -- \
  --model-dir models/supertonic-3 \
  --backend metal \
  --voice F1 \
  --language fr \
  --steps 8 \
  --text 'Bonjour depuis NobodyWho.' \
  --output bonjour.wav
```

Use `--backend cpu` on any supported target. Metal is available only on macOS builds.

## Scope

This is intentionally separate from the NobodyWho workspace. It currently supports:

- The `supertonic-3-orig.gguf` package only
- Batch size one
- CPU and Metal backends
- The ten built-in Supertonic voices
- The languages supported by Supertonic 3

It does not yet provide graph caching, long-text chunking, streaming, quantized weights, public bindings, or fallback between CPU and Metal. Graphs are rebuilt for each synthesis request, and the denoiser graph is rebuilt for every step so each iteration receives fresh input buffers.

## Implementation notes

The shared `../ggml-runtime` crate pins the same `llama-cpp-sys-2` revision used by NobodyWho. It provides raw GGML backend buffers, tensor graphs, graph allocation, and execution for this POC and the Mimir POC. The audio.cpp GGUF package stores logical names and exact tensor shapes in `audiocpp.*` metadata, which the loader resolves before uploading weights.

The graph formulas were ported from audio.cpp's Apache-2.0 Supertonic implementation and checked against NobodyWho’s ONNX implementation. Pass `--debug-dir PATH` to save stage tensors as JSON for numerical comparisons. See `THIRD_PARTY_NOTICES.md`.
