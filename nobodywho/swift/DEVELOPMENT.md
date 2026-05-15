# nobodywho — Swift bindings (UniFFI)

Swift Package for running LLMs locally on iOS, macOS, visionOS, and watchOS (CPU-only). Uses [UniFFI](https://mozilla.github.io/uniffi-rs/) to generate Swift bindings from Rust, distributed as an xcframework via Swift Package Manager.

## Prerequisites

- macOS with Xcode 16+
- Rust stable toolchain with Apple targets: `rustup target add aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin`
- Rust nightly toolchain for tier 3 targets (visionOS, watchOS): `rustup toolchain install nightly && rustup +nightly component add rust-src`

## Project structure

```
swift/
├── Package.swift                # SPM manifest — local dev uses path:, CI patches to url:+checksum:
├── DEVELOPMENT.md               # This file
├── README.md                    # Published to the mirror repo and Swift Package Index
│
├── src/                         # Hand-written Swift wrappers (the public API)
│   ├── Chat.swift               # Chat session (ask, streaming, history)
│   ├── Model.swift              # Model loading (local, hf://, https://)
│   ├── Encoder.swift            # Text embeddings
│   ├── CrossEncoder.swift       # Document reranking
│   ├── Tool.swift               # Tool calling (manual API + callback bridges)
│   ├── Prompt.swift             # Multimodal prompts (text, image, audio)
│   ├── TokenStream.swift        # AsyncSequence wrapper for streaming tokens
│   ├── SamplerPresets.swift     # Static factory methods for sampler configs
│   ├── Macros.swift             # @DeclareTool macro declaration
│   └── Exports.swift            # Re-exports from NobodyWhoGenerated
│
├── macros/                      # Swift compiler plugin (generates tool boilerplate)
│   └── ToolMacro.swift          # @DeclareTool peer macro implementation
│
├── generated/                   # Generated bindings (committed, regenerate when Rust API changes)
│   ├── nobodywho.swift          # Generated Swift classes, enums, free functions
│   ├── nobodywhoFFI.h           # C header for the FFI layer
│   └── nobodywhoFFI.modulemap   # Clang module map
│
├── scripts/
│   └── build-swift-xcframework.sh  # Build xcframework locally for development
│
├── Frameworks/                  # Local xcframework (gitignored, built by script above)
│   └── NobodyWhoNative.xcframework
│
└── tests/
    ├── MacroTests/              # @DeclareTool macro expansion tests (no model needed)
    │   └── ToolMacroTests.swift
    └── NobodyWhoTests/          # Integration tests (require TEST_MODEL env var)
        └── NobodyWhoTests.swift
```

## Architecture

The Swift package has four layers:

1. **NobodyWhoNative** — prebuilt xcframework containing the compiled Rust static library (`.a` files for each platform)
2. **NobodyWhoGenerated** — auto-generated Swift bindings from UniFFI (`nobodywho.swift`), with raw FFI types like `RustChat`, `RustModel`, etc.
3. **NobodyWho** — hand-written Swift wrappers that provide an idiomatic API (`Chat`, `Model`, `Tool`, etc.)
4. **NobodyWhoMacros** — Swift compiler plugin for the `@DeclareTool` macro

Users only `import NobodyWho`. The generated layer is not directly importable.

## When to regenerate bindings

### Generated Swift bindings (`generated/nobodywho.swift`, `generated/nobodywhoFFI.h`)

**Regenerate when:** the Rust API changes — adding/removing/renaming functions, types, errors, or changing signatures in `uniffi/src/lib.rs`.

```bash
# From nobodywho/ (workspace root)
cargo build -p nobodywho-uniffi
cargo run --bin uniffi-bindgen -- generate \
  --library target/debug/libnobodywho_uniffi.so \
  --language swift \
  --out-dir swift/generated
```

This reads UniFFI metadata from the compiled library and regenerates:
- `generated/nobodywho.swift` — Swift classes, enums, protocols, free functions
- `generated/nobodywhoFFI.h` — C header with FFI function declarations
- `generated/nobodywhoFFI.modulemap` — Clang module map

**Do not regenerate for:** Swift wrapper changes, sampler preset additions, README changes, version bumps.

### Swift wrappers (`src/*.swift`)

**Never regenerated** — these are hand-written. When the generated bindings change, you may need to update the wrappers to match (e.g., wrapping a new generated type).

### Quick reference

| What changed | Regenerate bindings | Update wrappers | Rebuild xcframework |
|---|---|---|---|
| Rust API (`uniffi/src/lib.rs`) | Yes | Maybe | Yes |
| Core Rust library (`core/src/`) | No | No | Yes |
| Swift wrappers (`src/*.swift`) | No | N/A | No |
| Macro (`macros/ToolMacro.swift`) | No | No | No |

## Local development

### Building the xcframework

For local development, you need a locally-built xcframework. The `Package.swift` references it at `Frameworks/NobodyWhoNative.xcframework`:

```bash
# From nobodywho/ (workspace root)
./swift/scripts/build-swift-xcframework.sh
```

This builds the Rust library for all 7 targets (iOS device/sim, macOS, visionOS device/sim, watchOS device/sim) and assembles them into an xcframework. Requires macOS with Xcode.

visionOS and watchOS targets require Rust nightly:
```bash
rustup toolchain install nightly
rustup +nightly component add rust-src
```

### Running tests

```bash
# Macro tests (no model needed)
swift test --filter NobodyWhoMacroTests

# Integration tests (requires a model)
TEST_MODEL=/path/to/model.gguf swift test --filter NobodyWhoTests

# Vision tests (requires vision model + mmproj)
TEST_MODEL=/path/to/model.gguf \
TEST_VISION_MODEL=/path/to/vision-model.gguf \
TEST_MMPROJ=/path/to/mmproj.gguf \
swift test --filter NobodyWhoTests
```

## Release pipeline

The Swift package uses a **mirror repo** pattern. The source of truth is in the monorepo at `nobodywho/swift/`, but SPM consumers install from the mirror at [nobodywho-ooo/nobodywho-swift](https://github.com/nobodywho-ooo/nobodywho-swift).

### How releases work

1. Push a tag like `nobodywho-swift-v0.2.0` to the monorepo
2. CI builds the xcframework for all platforms (iOS, macOS, visionOS, watchOS — device + simulator)
3. CI creates a GitHub Release on the monorepo with `NobodyWhoNative.xcframework.zip` attached
4. CI clones the mirror repo and:
   - Copies `src/`, `generated/`, `macros/`, `tests/`, `Package.swift`, `README.md` from the monorepo
   - **Patches `Package.swift`**: replaces the local `path:` binary target with a remote `url:` + `checksum:` pointing to the GitHub Release asset
   - Commits, tags with the version (e.g., `0.2.0`), and force-pushes to the mirror

### What this means in practice

- **`Package.swift` in the monorepo** always uses `path: "Frameworks/NobodyWhoNative.xcframework"` for local development
- **`Package.swift` in the mirror** gets patched by CI to use `url:` + `checksum:` for the release xcframework
- **You never manually edit the mirror repo** — it gets force-pushed on every release. All changes go through the monorepo
- **The README in the mirror** comes from `nobodywho/swift/README.md` — update it there

### Versioning

The Swift package version comes from the git tag: `nobodywho-swift-v0.2.0` → version `0.2.0` in SPM. The version is not stored in any file — it's derived from the tag.

The `uniffi/Cargo.toml` version is independent and does not need to match the Swift package version.

## Platform notes

### watchOS

- **CPU-only** — Apple Watch has no Metal for general compute
- The `llama-cpp-rs` fork handles this in `build.rs`: disables Metal linking, sets `-D_DARWIN_C_SOURCE` for BSD type compatibility, keeps Accelerate framework for vDSP operations
- Tier 3 Rust target — requires nightly + `-Z build-std`
- Untested at runtime — unclear if Apple Watch hardware can practically run LLM inference

### visionOS

- Metal GPU acceleration works (auto-enabled by llama-cpp-rs)
- Tier 3 Rust target — requires nightly + `-Z build-std`

### iOS / macOS

- Metal GPU acceleration works (auto-enabled by llama-cpp-rs)
- Stable Rust targets

## Known issues

- **Async tool callbacks use semaphore blocking:** `AsyncToolCallbackImpl` blocks the Rust inference thread with a `DispatchSemaphore` while a `Task` runs the async handler. This is safe because the inference thread is a plain OS thread (not in Swift's cooperative pool), but it's fragile if the threading model ever changes. See the safety comment in `Tool.swift`.

- **`@DeclareTool` scope limitation:** Swift peer macros can only introduce declarations at top-level or type-member scope, not inside function bodies. Use the manual `Tool` initializer for local-scope tools.

- **Generated bindings formatting:** `uniffi-bindgen` tries to run `swift-format` on the generated code. If `swift-format` is not installed, it prints a warning but the bindings are still correct.
