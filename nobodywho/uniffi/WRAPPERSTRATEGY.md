# UniFFI Wrapper Strategy

This document defines which types in the `nobodywho-uniffi` crate are intended to be wrapped by language-specific bindings (React Native, Swift, Kotlin, etc.) and which should be re-exported directly.

## Naming Convention

Types that **should be wrapped** in each target language are prefixed with `Rust` (e.g. `RustChat`, `RustModel`). The target language wrapper then provides the clean name (e.g. `Chat`, `Model`) with an ergonomic API. This makes it immediately obvious at the UniFFI level which types need a wrapper and which can be used as-is.

## Wrapped Types (`Rust*` prefix)

These types are intentionally thin FFI wrappers. Each target language must provide a wrapper class that hides the `Rust*` type and adds language-idiomatic ergonomics.

| UniFFI Type | Wrapper Name | Why Wrapped |
|-------------|-------------|-------------|
| `RustModel` | `Model` | Async factory (`Model.load()`), resource cleanup (`destroy()`), options-object API |
| `RustChat` | `Chat` | Convenience factory (`Chat.fromPath()`), accepts wrapper types (`Model`, `Tool`), returns `TokenStream` |
| `RustTokenStream` | `TokenStream` | Async iteration support (e.g. `AsyncIterable` in TS, `AsyncSequence` in Swift) |
| `RustTool` | `Tool` | Declarative parameter API with JSON schema generation, typed callback dispatch |
| `RustToolCallback` | (internal) | Implemented by the `Tool` wrapper internally, never exposed to consumers |
| `RustEncoder` | `Encoder` | Convenience factory (`Encoder.fromPath()`), resource cleanup |
| `RustCrossEncoder` | `CrossEncoder` | Convenience factory (`CrossEncoder.fromPath()`), `rankAndSort()` JSON parsing, resource cleanup |

### What each wrapper should provide

- **`Model`**: `Model.load(opts)` async factory (wraps `load_model` free function). `destroy()` for eager resource cleanup. Private constructor — consumers must use the factory.
- **`Chat`**: Constructor taking wrapper `Model` and `Tool` types. `Chat.fromPath(opts)` convenience that loads a model and creates a chat in one step. `ask()` returns a `TokenStream`. All other methods delegate to `RustChat`.
- **`TokenStream`**: Async iteration support for the target language. `nextToken()` and `completed()` methods.
- **`Tool`**: Accepts a user-friendly parameter definition (name, description, parameter schemas, callback). Internally constructs `RustTool` and implements `RustToolCallback` to parse JSON arguments and dispatch to the user's typed callback.
- **`Encoder`**: `Encoder.fromPath(opts)` factory. `encode(text)` returns embeddings. `destroy()`.
- **`CrossEncoder`**: `CrossEncoder.fromPath(opts)` factory. `rank()` and `rankAndSort()` (parses JSON tuples). `destroy()`.

## Direct Re-exports (no wrapper needed)

These types are ergonomic enough as generated and should be re-exported directly under their original names.

| UniFFI Type | Kind | Notes |
|-------------|------|-------|
| `SamplerConfig` | Object | Serializable via `toJson()`/`fromJson()`. Produced by `SamplerBuilder` and preset functions. |
| `SamplerBuilder` | Object | Fluent builder API, works well as-is in all languages. |
| `Role` | Enum | Simple enum: `User`, `Assistant`, `System`, `Tool`. |
| `Asset` | Record | Simple data: `{ id, path }`. |
| `ToolCall` | Record | Simple data: `{ name, argumentsJson }`. |
| `cosine_similarity` | Function | Pure utility function. |
| `sampler_preset_*` | Functions | May be collected into a static `SamplerPresets` class by the wrapper layer for ergonomics. |

## Hidden Types (internal to wrappers)

These types are used internally by the wrapper layer but should not be exposed to consumers. Each language should use its access control to hide them (TypeScript: `exports` field, Kotlin: `internal`, Swift: `package`).

| UniFFI Type | Kind | Hidden Because |
|-------------|------|---------------|
| `Message` | Enum | Awkward generated API (`Message.Message` stutter, tagged union). Wrapper converts to a flat type (e.g. `ChatMessage` in TS). |
| `PromptPart` | Enum | Implementation detail of `Prompt` wrapper class. |
| `ToolParameter` | Record | Implementation detail of `Tool` wrapper. Consumers declare parameters as a plain object/dict. |
| `load_model` | Function | Replaced by `Model.load()` factory. |

## Adding a New Type

When adding a new type to the UniFFI crate:

1. **Decide**: Does it need language-specific ergonomics (async factories, iteration support, type conversions, resource cleanup)? If yes, prefix it with `Rust` and add a wrapper in each binding.
2. **If wrapping**: Add the `Rust`-prefixed type here, document what the wrapper should provide, and implement the wrapper in each target language.
3. **If not wrapping**: Add it to the "Direct Re-exports" table and export it as-is from each binding's public API.
4. **If internal-only**: Add it to the "Hidden Types" table and ensure each language hides it from consumers.
