# Third-party notices

## audio.cpp

The Supertonic graph formulas and GGUF metadata handling in this POC were adapted from the Supertonic implementation in [audio.cpp](https://github.com/0xShug0/audio.cpp), copyright 2026 ShugoAI LLC, licensed under the Apache License 2.0.

The implementation was ported to Rust, reduced to CPU and Metal, and changed to execute through direct GGML bindings. The Apache License 2.0 text is included at `LICENSES/Apache-2.0.txt`.

## GGML

The shared runtime builds [GGML](https://github.com/ggml-org/ggml) directly at commit `d4716378882593333721eb33f153144b6885caf2`. GGML is licensed under the MIT License. Its license is included in the `../ggml-sys/ggml` submodule.

## Supertonic 3

Model configuration and voice assets come from [Supertone/supertonic-3](https://huggingface.co/Supertone/supertonic-3). The converted GGUF weights come from [audio-cpp/audio.cpp-gguf](https://huggingface.co/audio-cpp/audio.cpp-gguf).

Supertonic 3 model weights are licensed under the BigScience Open RAIL-M license. The weights are downloaded separately and are not included in this repository. The model license is included at `LICENSES/Supertonic-3-Open-RAIL-M.txt`.

## Rust dependencies

Other Rust dependencies retain their upstream licenses. `Cargo.lock` identifies the exact resolved versions.
