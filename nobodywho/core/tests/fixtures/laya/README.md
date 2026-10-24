# Laya reference cases

`cases.json` contains two requests and their llama.cpp responses:

- `reference`: all three question types.
- `structured_json`: nested values, Unicode, and option order.

- Model: `ggml-org/Laya-GGUF`, revision `845f314dd047fc2444a5289a4d50c1e1d59e27e1`, `Laya-Q8_0.gguf`.
- SHA-256: `c06528c5746d3bb8baa72a27938be95abbfd0b226f8471e8a9e365ed0bb066d2`.
- llama.cpp: `d81235049384534c167caea52b85a694f6103d14`.
- Server flags: `-c 1024 -b 1024 -ub 1024 -t 12 -tb 12 -ngl 0 -np 1`.

One slot evaluates questions separately, matching the core worker. The machine-specific `model` field is omitted.

The responses were captured on ARM macOS. Q8 inference varies across CPU backends, so model-reference probabilities, confidence, and scores allow `0.05` absolute error. Labels and token counts remain exact. Sync/async comparisons keep a tighter `1e-4` tolerance; the softmax unit test checks the math independently.

Run from `nobodywho/`:

```sh
TEST_DECISION_MODEL=/path/to/Laya-Q8_0.gguf cargo test -p nobodywho real_laya_matches_llama_cpp
```
