# GGML Mimir POC

A standalone Rust inference engine for [DFM Mimir](https://huggingface.co/danish-foundation-models/DFM-Mimir) using the raw GGML API exposed by `llama-cpp-sys-2`. It runs the unsupported HRM-Text architecture on CPU and Apple Metal without Transformers, PyTorch, ONNX Runtime, or llama.cpp's model API.

The implementation includes:

- Direct loading of the original BF16 safetensors checkpoint
- Hugging Face tokenizer loading
- Prefix-LM attention masks
- Packed gate/query/key/value projections
- Gated multi-head attention with RoPE
- SwiGLU feed-forward blocks
- Two high-level and three low-level recurrent cycles
- Greedy English and Danish text generation

## Run

The model download is about 3.6 GB.

```sh
cd poc/ggml-mimir
make model
make run
```

Use CPU explicitly:

```sh
make run-cpu PROMPT='Hvad er Danmarks hovedstad?' MAX_TOKENS=16
```

Or use Cargo directly:

```sh
cargo run --release -- \
  --model-dir models/DFM-Mimir \
  --backend metal \
  --prompt 'Explain why the sky is blue.' \
  --max-tokens 32
```

Metal is available only on macOS. CPU is supported on other targets built by `llama-cpp-sys-2`.

## Validation

The prompt `Hi` was compared against Transformers 5.15.1 using the same BF16 checkpoint and prefix-LM mask.

| Check | Result |
| --- | ---: |
| Transformers versus Metal first-token correlation | 0.9999981 |
| Transformers versus CPU first-token correlation | 0.9999979 |
| Eight-token greedy output | Exact match |
| Metal, eight tokens | 0.67 s |
| CPU with eight threads, four tokens | 16.90 s |
| Peak Metal process RSS | 7.03 GiB |

Both implementations generated `Hello! How can I help you today`. A Danish smoke test generated `Danmarks hovedstad er København.`

These timings were measured on an Apple M3 Max. They exclude model loading, which took about 0.5 seconds from the local filesystem.

## Scope and limitations

This is an architecture-validation POC, not a production LLM runtime.

- Generation is greedy only.
- There is no KV cache. Every token recomputes the full sequence.
- The prompt is rendered as one user turn using Mimir's basic chat format.
- Tool calls, system messages, sampling, batching, and streaming are not implemented.
- The original BF16 checkpoint is used directly. GGUF conversion and quantization are not implemented.
- Although the model declares a 4,096-token context, short contexts are more practical without a KV cache.
- CPU inference is much slower than Metal.

Long-term support belongs in llama.cpp so Mimir can reuse its tokenizer, sampler, KV cache, quantization, batching, and public model API.

## Shared runtime

The generic GGML code lives in `../ggml-runtime` and is also used by the Supertonic POC. It owns backend initialization, tensor and shape handling, GGUF and safetensors loading, graph construction, common operations, allocation, execution, and tensor transfers. Model architecture code remains inside each POC.

See `THIRD_PARTY_NOTICES.md` for licenses and source references.
