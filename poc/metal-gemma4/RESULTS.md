# Gemma 4 E2B Metal results

We tested Gemma 4 E2B Q4_K_M on an Apple M3 Max.

- Greedy decoding with F16 KV caches
- 12 prompts and eight paired runs per prompt
- 384 measurements for the Metal comparisons
- 192 additional measurements for Direct GGML vs llama.cpp
- Exact token parity in every run

## Implementations

| Name | What it is |
|---|---|
| llama.cpp | The standard llama.cpp runtime using its Metal backend. |
| Direct GGML | Our first implementation, built on GGML's compute graph and Metal backend. |
| Serial Metal | Our custom Metal rewrite before the SIMD attention optimization. |
| Metal | Builds on Serial Metal and adds SIMD attention. GGML only reads the GGUF file and converts two embedding buffers. |

## Results

| Comparison | Median speedup | 95% confidence interval | Prompt wins |
|---|---:|---:|---:|
| Metal vs Serial Metal | 6.23% | 4.74% to 8.27% | 12/12 |
| Metal vs Direct GGML | 15.45% | 8.41% to 17.79% | 12/12 |
| **Metal vs llama.cpp** | **9.57%** | 6.41% to 11.67% | 12/12 |
| Direct GGML vs llama.cpp | -5.58% | -5.94% to -4.96% | 1/12 |

## Metal implementation

- GGML reads the GGUF file and converts two embedding buffers to F16. It does not run inference.
- Custom Metal kernels read Q4_K, Q5_K, and Q6_K weights directly.
- The runtime preallocates F32 activations and F16 KV caches.
- Fused kernels combine normalization, residual addition, and scaling.
- Each attention dot product uses 32 GPU lanes.
- One Metal command buffer runs each token pass and greedy argmax.

## Transferability

- **Can this be used with other backends?** The approach can. The code cannot. CUDA, Vulkan, and other backends need their own kernels and command scheduling.
- **Does it transfer well to other models?** The model runner is specific to Gemma 4 E2B. Another model needs its own tensor names, dimensions, layer order, KV layout, positional encoding, and FFN rules.
