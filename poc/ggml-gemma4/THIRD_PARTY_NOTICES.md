# Third-party notices

## Gemma 4 E2B

The configuration, tokenizer, and generation configuration are downloaded from [google/gemma-4-E2B-it](https://huggingface.co/google/gemma-4-E2B-it) at revision `3e22461f65e89153144f8adb70e3b8c2cc9845a7`.

The Q4_K_M GGUF is downloaded from [unsloth/gemma-4-E2B-it-GGUF](https://huggingface.co/unsloth/gemma-4-E2B-it-GGUF) at revision `0314792d7f1f7e229411f620751375812bb9faf2`.

Gemma 4 is licensed under the Apache License 2.0. The downloaded assets are ignored by Git and are not included in this repository. The license text is included at `LICENSES/Apache-2.0.txt`.

## llama.cpp and llama-cpp-rs

The Gemma 4 graph follows the Apache-2.0 llama.cpp implementation bundled with `llama-cpp-sys-2`.

The shared runtime consumes `llama-cpp-sys-2` from [utilityai/llama-cpp-rs](https://github.com/utilityai/llama-cpp-rs) at revision `bed81ad4ab1a6c904b11d425608e50f976d8ea62`. It builds and links GGML from llama.cpp. Their respective licenses and bundled third-party notices apply.

## Rust dependencies

Other Rust dependencies retain their upstream licenses. `Cargo.lock` identifies the exact resolved versions.
