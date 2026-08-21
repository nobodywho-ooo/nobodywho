# nobodywho — Kotlin/Android bindings (UniFFI)

Android library for running LLMs locally with Vulkan/Metal GPU acceleration. Uses [UniFFI](https://mozilla.github.io/uniffi-rs/) to generate JNA bindings from Rust.

## Prerequisites

- Rust toolchain (stable) with Android targets: `rustup target add aarch64-linux-android x86_64-linux-android`
- Nix (`nix develop .#android` provides NDK r28, Android SDK, JDK 17, Gradle, and all cross-compilation env vars)
- For running integration tests: a GGUF model file (e.g. Qwen3-0.6B)

## Project structure

```
kotlin/
├── build.gradle.kts             # Android library build config + maven publishing
├── settings.gradle.kts          # Gradle settings (plugin versions)
├── gradle/                      # Gradle wrapper
├── src/                         # Hand-written Kotlin wrappers
│   ├── Chat.kt                  # Chat session (ask, streaming, tools, history)
│   ├── Model.kt                 # Model loading (local path, URL, HuggingFace)
│   ├── Tool.kt                  # Tool via KFunction reflection
│   ├── TokenStream.kt           # Coroutine Flow for streaming
│   ├── Encoder.kt               # Embedding generation
│   ├── CrossEncoder.kt          # Document reranking
│   ├── Prompt.kt                # Multimodal prompts (text, image, audio)
│   ├── SamplerPresets.kt        # Factory methods for common sampler configs
│   ├── SamplerDsl.kt            # DSL builder for sampler configs
│   ├── SchemaUtils.kt           # JSON Schema generation from Kotlin types
│   ├── Utils.kt                 # Cosine similarity helper
│   ├── Exports.kt               # Re-exports and Message wrapper (see below)
│   └── main/AndroidManifest.xml
├── generated/                   # Generated bindings (committed, regenerate when Rust API changes)
│   └── uniffi/nobodywho/
│       └── nobodywho.kt
└── test/                        # Tests
    ├── SchemaUtilsTest.kt       # Unit tests for JSON Schema generation
    ├── SamplerDslTest.kt        # Unit tests for sampler DSL builder
    └── IntegrationTest.kt       # Integration tests (requires native lib + model)
```

## When to regenerate bindings

**Regenerate when:** The Rust API changes — adding/removing/renaming functions, types, errors, or changing their signatures in `uniffi/src/lib.rs`.

```bash
# From nobodywho/ (workspace root)
cargo build -p nobodywho-uniffi
cargo run --bin uniffi-bindgen -- generate \
  --library target/debug/libnobodywho_uniffi.so \
  --language kotlin \
  --out-dir kotlin/generated
```

**After regenerating:** Check that the wrapper classes in `src/` still match the generated API. In particular:
- `SamplerPresets.kt` — new sampler preset functions may need wrapping
- `Exports.kt` — new data types used in public APIs may need type aliases
- Import aliases in wrapper files (`RustChat`, `RustModel`, etc.) — names may change

**Do not regenerate for:** Kotlin wrapper changes, build config changes, version bumps.

## Running tests locally

All tests run on the host JVM (not an Android emulator). The native library must be built for the host platform first.

```bash
# From nobodywho/ (workspace root)

# 1. Build the native library for the host
cargo build -p nobodywho-uniffi

# 2. Run tests (from project root, where flake.nix is)
nix develop .#android --command bash -c \
  'cd nobodywho/kotlin && \
   export NOBODYWHO_LIB_DIR="$(cd ../target/debug && pwd)" && \
   export TEST_MODEL=/path/to/model.gguf && \
   ./gradlew test'
```

- `NOBODYWHO_LIB_DIR` — directory containing `libnobodywho_uniffi.so` (sets `jna.library.path`)
- `TEST_MODEL` — path to a GGUF model file (integration tests are skipped if unset)

Unit tests (`SchemaUtilsTest`, `SamplerDslTest`) also require the native lib since we removed the uniffi stubs.

## Architecture notes

### Public API package: `ai.nobodywho`

All consumer-facing types live in `ai.nobodywho`. The generated UniFFI bindings are in `uniffi.nobodywho` and should not be imported by consumers directly.

Types that are part of the public API are either:
- Wrapper classes (`Chat`, `Model`, `Tool`, `Encoder`, etc.)
- Type aliases in `Exports.kt` (`SamplerConfig`, `Asset`, `ToolCall`)
- The `Message` sealed class in `Exports.kt` (wraps `uniffi.nobodywho.Message`)

### Why Message is wrapped

The generated `uniffi.nobodywho.Message` sealed class has a `Tool` variant, which clashes with the top-level `ai.nobodywho.Tool` class. Kotlin type aliases don't expose nested types, so `typealias Message = uniffi.nobodywho.Message` doesn't let consumers write `Message.Tool`.

The solution is a hand-written `Message` sealed class in `ai.nobodywho` that mirrors the generated one. `Chat.getChatHistory()` and `Chat.setChatHistory()` convert at the boundary.

### Tool function limitations

The `Tool` class uses Kotlin reflection (`KFunction`) to introspect parameter names and types. This requires the function to be a top-level function, a class method, or a companion object method. Local functions defined inside other functions or coroutines are not supported — the Kotlin compiler mangles their JVM signatures, which breaks reflection.

## Building native libraries for Android

For local development with an Android device/emulator:

```bash
# From project root (where flake.nix is)
nix develop .#android --command bash -c \
  'cd nobodywho && cargo build -p nobodywho-uniffi --target aarch64-linux-android --release'
```

The `.so` files need to be placed in `build/jniLibs/{arm64-v8a,x86_64}/` for the Gradle build to pick them up.
