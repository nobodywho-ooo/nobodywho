# Third-party notices

## DFM Mimir

Model weights, configuration, tokenizer, and chat template are downloaded from [danish-foundation-models/DFM-Mimir](https://huggingface.co/danish-foundation-models/DFM-Mimir) at revision `2844f0178e695d7d9ce182cb660671fd34c76ce5`.

DFM Mimir is licensed under the Apache License 2.0. The downloaded assets are ignored by Git and are not included in this repository. The license text is included at `LICENSES/Apache-2.0.txt`.

## HRM-Text

The graph structure and packed tensor layout were implemented from the Apache-2.0 [schneiderkamplab/HRM-Text](https://github.com/schneiderkamplab/HRM-Text) source and the Hugging Face `hrm_text` implementation.

## llama.cpp and llama-cpp-rs

The shared runtime consumes `llama-cpp-sys-2` from [utilityai/llama-cpp-rs](https://github.com/utilityai/llama-cpp-rs) at revision `bed81ad4ab1a6c904b11d425608e50f976d8ea62`. It builds and links GGML from llama.cpp. Their respective licenses and bundled third-party notices apply.

## Rust dependencies

Other Rust dependencies retain their upstream licenses. `Cargo.lock` identifies the exact resolved versions.
