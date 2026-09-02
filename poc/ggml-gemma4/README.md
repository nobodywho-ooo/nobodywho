# GGML Gemma 4 Metal benchmark

A standalone Gemma 4 E2B benchmark using direct GGML bindings, without llama.cpp's model API. It loads the Q4_K_M GGUF and runs fixed prompt-processing and token-generation workloads on Apple Metal.

The benchmark uses:

- one reusable graph for prompt processing
- one reusable single-token decode graph
- device-resident F16 KV caches
- fused Metal flash attention with native grouped-query attention by default
- shared KV states for the final Gemma 4 layers
- no tokenization, sampling, text decoding, or logits download in timed sections
- a preflight greedy-token parity check against llama.cpp
- separate `pp512` and `tg128` measurements by default

## Setup

Initialize the pinned GGML source:

```sh
git submodule update --init poc/ggml-sys/ggml
```

Install `hf`, `jq`, `pkg-config`, a C++ compiler, and llama.cpp with `llama-bench` and its development library. The script resolves both `libllama` and `llama-bench` from the same pkg-config installation. Then download the pinned model:

```sh
cd poc/ggml-gemma4
make model
```

The model is about 3.1 GB and is stored in `models/gemma-4-E2B-it` at the repository root.

## Verify inference

Run greedy generation through both implementations and print their token IDs, decoded completions, total generation latency, and throughput:

```sh
./verify.sh
```

The command completes `The capital of Denmark is`, stops at EOS, and checks exact token parity. It uses at most 32 generated tokens and flash attention by default. Override these with `PROMPT='The capital of Portugal is'`, `TOKENS=64`, or `FLASH_ATTN=off`. Timing includes prompt ingestion and generation. It excludes model loading, one warmup token, tokenization, and text decoding.

The equivalent Make target is `make verify`.

## Side-by-side benchmark

```sh
make benchmark
```

Run this on an otherwise idle Mac because other Metal workloads can materially affect both results. Before timing, the command greedily generates eight token IDs from the model's BOS token with both implementations and exits if they differ. This validation downloads logits but is outside all timed sections. Timed graph calls synchronize Metal before recording their duration.

The command then compares the direct GGML implementation with `llama-bench` using the same:

- Gemma 4 E2B Q4_K_M file
- prompt and generation token counts
- F16 KV cache
- Metal offload
- flash-attention setting, enabled by default
- repetition count

Override the workload with Make variables:

```sh
make benchmark PROMPT_TOKENS=256 GENERATION_TOKENS=64 GREEDY_TOKENS=8 REPETITIONS=3
```

Run only the direct GGML benchmark:

```sh
cargo run --release -- \
  --model-dir ../../models/gemma-4-E2B-it \
  --prompt-tokens 512 \
  --generation-tokens 128 \
  --repetitions 5 \
  --flash-attention
```

Pass `--json` for machine-readable throughput output. Set `FLASH_ATTN=off` for the matched non-flash baseline. Set `OUTPUT_DIR` when running `benchmark-metal.sh` to retain the throughput results, both greedy token sequences, and the llama.cpp validation log.

## Scope

This benchmark supports only the pinned Gemma 4 E2B instruction-tuned Q4_K_M model on macOS Metal. It is not a chat or text-generation CLI. Production Gemma 4 support remains in NobodyWho through llama.cpp.

The generic GGML code lives in `../ggml-runtime`. It owns backend initialization, tensor storage, GGUF loading, graph construction, allocation, execution, and transfers. The Gemma architecture and benchmark graphs remain in this crate.
