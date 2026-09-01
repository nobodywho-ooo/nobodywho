# Third-party notices

## Gemma 4 E2B

The configuration, tokenizer, and generation configuration are downloaded from [google/gemma-4-E2B-it](https://huggingface.co/google/gemma-4-E2B-it) at revision `3e22461f65e89153144f8adb70e3b8c2cc9845a7`.

The Q4_K_M GGUF is downloaded from [unsloth/gemma-4-E2B-it-GGUF](https://huggingface.co/unsloth/gemma-4-E2B-it-GGUF) at revision `0314792d7f1f7e229411f620751375812bb9faf2`.

Gemma 4 is licensed under the Apache License 2.0. The downloaded assets are ignored by Git and are not included in this repository. The license text is included at `LICENSES/Apache-2.0.txt`.

## llama.cpp

The Gemma 4 graph follows the Apache-2.0 [llama.cpp](https://github.com/ggml-org/llama.cpp) implementation at commit `5f55650a78f92aff4d48d671423e888fac0469ff`.

## GGML

The shared runtime builds [GGML](https://github.com/ggml-org/ggml) directly at commit `d4716378882593333721eb33f153144b6885caf2`. GGML is licensed under the MIT License. Its license is included in the `../ggml-sys/ggml` submodule.

## Rust dependencies

Other Rust dependencies retain their upstream licenses. `Cargo.lock` identifies the exact resolved versions.
