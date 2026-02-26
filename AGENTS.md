# AGENTS.md

Essential information for AI agents working on the NobodyWho codebase.

## Project Overview

NobodyWho is a Rust-based library for running LLMs locally with offline inference. Core features include streaming responses, tool calling, and context management. Built on the `llama-cpp-2` crate.

## Architecture

### Core Rust Library

The main implementation is in `nobodywho/core/src/`:

- [`chat.rs`](nobodywho/core/src/chat.rs) - Chat API with conversation management (main file, 3000+ lines)
- [`llm.rs`](nobodywho/core/src/llm.rs) - Model loading and worker management
- [`encoder.rs`](nobodywho/core/src/encoder.rs) - Embeddings generation
- [`crossencoder.rs`](nobodywho/core/src/crossencoder.rs) - Cross-encoder for reranking
- [`errors.rs`](nobodywho/core/src/errors.rs) - Error types using `thiserror`
- [`sampler_config.rs`](nobodywho/core/src/sampler_config.rs) - Sampling configuration

### Language Bindings

- **Python** ([`nobodywho/python/`](nobodywho/python/)) - PyO3/maturin bindings
- **Godot** ([`nobodywho/godot/`](nobodywho/godot/)) - GDExtension bindings
- **Flutter** ([`nobodywho/flutter/`](nobodywho/flutter/)) - FFI bindings via `flutter_rust_bridge`

## Key Types & Patterns

### Core Types

- `ChatHandle` / `ChatHandleAsync` - Main chat interface (sync and async)
- `ChatBuilder` - Builder pattern for chat configuration
- `Message` enum - User/Assistant/System/Tool messages
- `Model` - Shared model instance (`Arc<LlamaModel>`)
- `Worker` - Background task for model inference

### Error Handling

Uses `thiserror` crate for error types. All errors are defined in [`errors.rs`](nobodywho/core/src/errors.rs) and implement `std::error::Error`. Common error types include `LoadModelError`, `InitWorkerError`, `ChatWorkerError`.

### Key Dependencies

- `llama-cpp-2` - underlying LLM inference engine
- `tokio` - Async runtime
- `serde` / `serde_json` - Serialization
- `minijinja` - Template rendering for chat templates
- `gbnf` - Grammar-based tool calling
- `tracing` - Logging framework

## Build & Test

### Building

**Core library:**
```bash
cd nobodywho
cargo build
```

**Python bindings:**
```bash
cd nobodywho/python
maturin develop --uv
cargo run --bin make_stubs  # Generate type stubs
```

### Testing

**Core tests:**
```bash
cd nobodywho
export TEST_MODEL=/path/to/model.gguf
cargo test -- --nocapture --test-threads=1
```

**Python tests:**
```bash
cd nobodywho/python
pytest  # Also tests markdown documentation code blocks
```

### Development Environment

- **Linux/WSL:** Use Nix flakes (`nix develop`)
- **Windows:** Install rustup, cmake, llvm, msvc, and Vulkan SDK

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for detailed setup instructions.

## Development Notes

### Platform Support

- Supported: Windows, Linux, macOS, Android
- GPU acceleration: Vulkan (x86/x86_64), Metal (macOS/iOS)
- Future: iOS support planned (see issue #114)

### Integration Patterns

**Python:**
- Use `#[pyclass]` for classes and `#[pymethods]` for methods
- See [`nobodywho/python/src/lib.rs`](nobodywho/python/src/lib.rs) for examples

**Godot:**
- Use `#[derive(GodotClass)]` and `#[godot_api]`
- See [`nobodywho/godot/src/lib.rs`](nobodywho/godot/src/lib.rs) for examples

**Flutter:**
- Uses `flutter_rust_bridge` for FFI bindings
- See [`nobodywho/flutter/rust/src/lib.rs`](nobodywho/flutter/rust/src/lib.rs) for examples

### Code Patterns

- Use `Arc<LlamaModel>` for shared model instances
- Builder pattern for configuration (`ChatBuilder`)
- Async support via `tokio` (`ChatHandleAsync`)
- Error propagation with `?` operator
- Tracing for logging (`tracing::info!`, `tracing::debug!`, etc.)

## Important Files

- [`nobodywho/core/src/chat.rs`](nobodywho/core/src/chat.rs) - Main chat API
- [`nobodywho/core/src/llm.rs`](nobodywho/core/src/llm.rs) - Model and worker management
- [`nobodywho/core/Cargo.toml`](nobodywho/core/Cargo.toml) - Core dependencies
- [`nobodywho/python/src/lib.rs`](nobodywho/python/src/lib.rs) - Python bindings
- [`nobodywho/godot/src/lib.rs`](nobodywho/godot/src/lib.rs) - Godot extension
- [`CONTRIBUTING.md`](CONTRIBUTING.md) - Development setup
- [`README.md`](README.md) - User-facing documentation

## Documentation
Documentation is available under the `docs/` folder, but also at: docs.nobodywho.ooo.

