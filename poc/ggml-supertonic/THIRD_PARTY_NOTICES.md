# Third-party notices

## audio.cpp

The Supertonic graph formulas and GGUF metadata handling in this POC were adapted from the Supertonic implementation in [audio.cpp](https://github.com/0xShug0/audio.cpp), copyright 2026 ShugoAI LLC, licensed under the Apache License 2.0.

The implementation was ported to Rust, reduced to CPU and Metal, and changed to execute through `llama-cpp-sys-2`. The Apache License 2.0 text is included at `LICENSES/Apache-2.0.txt`.

## llama.cpp and llama-cpp-rs

`llama-cpp-sys-2` is consumed from [utilityai/llama-cpp-rs](https://github.com/utilityai/llama-cpp-rs) at revision `bed81ad4ab1a6c904b11d425608e50f976d8ea62`. It builds and links GGML from llama.cpp. Their respective license and bundled third-party notices apply.

## Supertonic 3

Model configuration and voice assets come from [Supertone/supertonic-3](https://huggingface.co/Supertone/supertonic-3). The converted GGUF weights come from [audio-cpp/audio.cpp-gguf](https://huggingface.co/audio-cpp/audio.cpp-gguf).

Supertonic 3 model weights are licensed under the BigScience Open RAIL-M license. The weights are downloaded separately and are not included in this repository. The model license is included at `LICENSES/Supertonic-3-Open-RAIL-M.txt`.

## Rust dependencies

Other Rust dependencies retain their upstream licenses. `Cargo.lock` identifies the exact resolved versions.
