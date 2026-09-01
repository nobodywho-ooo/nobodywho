# GGML Gemma 4 E2B POC

A standalone Rust text-generation engine for [Gemma 4 E2B](https://huggingface.co/google/gemma-4-E2B-it) using direct GGML bindings. It loads the Q4_K_M GGUF directly and runs on CPU or Apple Metal without llama.cpp's model API.

The implementation covers the E2B text decoder:

- Q4_K_M GGUF weights and Hugging Face tokenization
- Sliding and full causal attention
- Shared key/value states
- Per-layer token embeddings
- Gemma 4 proportional RoPE
- Double-width feed-forward layers
- Greedy text generation

## Run

The model download is about 3.1 GB.

```sh
cd poc/ggml-gemma4
make model
make run
```

Use CPU explicitly:

```sh
make run-cpu PROMPT='Explain why the sky is blue.' MAX_TOKENS=8
```

Or use Cargo directly:

```sh
cargo run --release -- \
  --model-dir models/gemma-4-E2B-it \
  --backend metal \
  --prompt 'Explain why the sky is blue.' \
  --max-tokens 8
```

Metal is available only on macOS. The direct GGML build uses the CPU backend on other targets.

## Validation

For the prompt `Hi`, the first four greedy tokens match the pinned llama.cpp implementation exactly: `Hi! How can`.

## Scope and limitations

This is an architecture POC, not a production LLM runtime.

- Generation is greedy only.
- There is no persistent KV cache. Every token recomputes the full sequence.
- The prompt is rendered as one user turn without tools, system messages, images, or audio.
- Sampling, batching, and streaming are not implemented.
- Only the Gemma 4 E2B instruction-tuned Q4_K_M file is supported.
- Short contexts are more practical despite the model's larger declared context window.

Production Gemma 4 support remains in NobodyWho through llama.cpp. This POC exists to exercise the shared raw GGML runtime.

## Shared runtime

The generic GGML code lives in `../ggml-runtime` and is also used by the Supertonic POC. It owns backend initialization, tensor and shape handling, GGUF loading, graph construction, common operations, allocation, execution, and tensor transfers. Model architecture code remains inside each POC.

See `THIRD_PARTY_NOTICES.md` for licenses and source references.
