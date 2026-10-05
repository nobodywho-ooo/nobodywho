# nobodywho — C# bindings (UniFFI)

.NET 10 library for running LLMs locally on Windows, Linux and macOS. It uses
[uniffi-bindgen-cs](https://github.com/NordSecurity/uniffi-bindgen-cs) to generate C# bindings from
the same `nobodywho-uniffi` crate that Swift, Kotlin and React Native use, and ships as the `NobodyWho`
NuGet package.

## Prerequisites

- Rust stable toolchain, plus what the workspace needs to build llama.cpp (see [`CONTRIBUTING.md`](../../CONTRIBUTING.md); `nix develop` provides all of it, including the .NET SDK)
- .NET 10 SDK

Or use the dev container, which has all of the above plus the pinned uniffi-bindgen-cs. From the
repository root:

```bash
docker build -t nobodywho-csharp-dev -f nobodywho/csharp/dev.Dockerfile .
docker run --rm -it -v "$PWD:/src" -w /src/nobodywho nobodywho-csharp-dev
```
- For the integration tests: GGUF models (see [Running tests](#running-tests))

## Project structure

```
csharp/
├── NobodyWho.slnx
├── dev.Dockerfile               # Build-and-test container, an alternative to nix develop
├── global.json                  # .NET 10 SDK, Microsoft.Testing.Platform test runner
├── uniffi.toml                  # uniffi-bindgen-cs config (namespace, internal access)
├── README.md                    # Published as the NuGet package readme
├── scripts/
│   └── generate-bindings.sh     # Regenerates src/NobodyWho/generated/
├── src/NobodyWho/
│   ├── NobodyWho.csproj
│   ├── generated/nobodywho.cs   # Generated, committed; regenerate when the Rust API changes
│   ├── Chat.cs                  # Chat session (ask, complete, history, settings)
│   ├── Model.cs                 # Model loading and downloading
│   ├── TokenStream.cs           # IAsyncEnumerable token streams (chat and speech-to-text)
│   ├── Tool.cs                  # Tools from delegates, JSON schema from parameter types
│   ├── Prompt.cs                # Multimodal and JSON prompts
│   ├── Messages.cs              # Message, MessageContent, ContentPart, ToolCall
│   ├── Types.cs                 # Option records, NobodyWhoException, native-call helpers
│   ├── Sampler.cs               # SamplerConfig, SamplerBuilder, SamplerPresets
│   ├── Encoder.cs               # Encoder (embeddings) and CrossEncoder (reranking)
│   └── Speech.cs                # TextToSpeech, SpeechToText, VoiceActivityDetection
├── samples/NobodyWho.Demo/      # Blazor app showing every capability (see its README)
└── tests/NobodyWho.Tests/       # xUnit v3
```

## Architecture

There are two layers:

1. **`NobodyWho.Native`** (`generated/`): the raw UniFFI bindings, with types like `RustChat` and
   `RustModel`. Everything here is `internal`, so it never shows up in the public API.
2. **`NobodyWho`**: hand-written wrappers that make up the whole public API, following
   [`WRAPPERSTRATEGY.md`](../uniffi/WRAPPERSTRATEGY.md). Because C# has no public type aliases, even
   the types other bindings re-export directly (records such as `ChatStats`, the sampler types) get a
   small public wrapper here.

Conventions in the wrapper layer:

- Every async Rust method becomes a `Task`-returning `…Async` method. The generated futures cannot
  be cancelled, so `CancellationToken`s are honoured in the wrapper: token streams call
  `StopGeneration`, and loads stop waiting and free the result when it arrives.
- Native errors are rethrown as `NobodyWhoException` by the `Checked` helpers in `Types.cs`.
- Counts and sizes are `int` in the public API and checked before they become `uint`.
- Callbacks into C# (tools, download progress) must never throw into native code. Tool exceptions
  are returned to the model as `Error: …`.

## Native library loading

The generated code P/Invokes `nobodywho_uniffi`. .NET finds it:

- **From the NuGet package**, under `runtimes/<rid>/native/`. The release workflow places the
  prebuilt libraries in `src/NobodyWho/runtimes/` (gitignored) before `dotnet pack`.
- **In local development**, `NobodyWho.csproj` copies the workspace's debug build
  (`nobodywho/target/debug/(lib)nobodywho_uniffi.{so,dylib,dll}`) next to the assembly. Point it
  elsewhere with `-p:NobodyWhoNativeLib=/path/to/lib`.

## Building

```bash
# From nobodywho/ (workspace root)
cargo build -p nobodywho-uniffi
cd csharp
dotnet build
```

## When to regenerate bindings

**Regenerate when** the Rust API in `uniffi/src/lib.rs` changes.

```bash
# From nobodywho/ (workspace root)
cargo build -p nobodywho-uniffi
bash csharp/scripts/generate-bindings.sh target/debug/libnobodywho_uniffi.so   # .dylib on macOS
```

`just regen-uniffi` does this together with the other UniFFI bindings, and CI fails if the committed
file is out of date.

The script installs the pinned uniffi-bindgen-cs into `nobodywho/target/tools` on first use. It is
pinned to the last commit that targets UniFFI 0.30, which the workspace uses. That version has one
known code-generation bug, patched by the script (jagged-array allocation), and the script also
disables warnings in the generated file. Both workarounds can go once the workspace moves to UniFFI
0.31 and the tagged uniffi-bindgen-cs v0.11 release.

**After regenerating**, check that the wrappers still match the generated API, and wrap any new
public types or functions.

## Running tests

```bash
cd nobodywho/csharp
dotnet test
```

Tests that need no model always run. Integration tests skip unless their model is set:

| Variable | Model used in CI |
|----------|------------------|
| `TEST_MODEL` | [Qwen3-0.6B Q4_K_M](https://huggingface.co/NobodyWho/Qwen_Qwen3-0.6B-GGUF) |
| `TEST_EMBEDDINGS_MODEL` | [bge-small-en-v1.5 Q8_0](https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf) |
| `TEST_CROSSENCODER_MODEL` | [bge-reranker-v2-m3 Q8_0](https://huggingface.co/gpustack/bge-reranker-v2-m3-GGUF) (not run in CI) |

Check formatting with `dotnet format NobodyWho.slnx --verify-no-changes` (also run by
`just csharp-build`).

## Demo app

[`samples/NobodyWho.Demo`](samples/NobodyWho.Demo/README.md) is a Blazor Server app with one page per
capability, each beside the code it runs. It is part of the solution, so CI builds it and checks
its formatting; run it with `dotnet run` from its folder.

## Releasing

C# is released like the other bindings (see the release skill): `just prepare-release` turns the
pending `.changeset/` files that name `csharp` into the next version, written to `<Version>` in
`NobodyWho.csproj`. Pushing the `nobodywho-csharp-v<version>` tag makes the release workflow pack the
NuGet package with the native libraries for `win-x64`, `linux-x64`, `linux-arm64` and `osx-arm64`,
attach it to a GitHub release and push it to nuget.org. That needs a `NUGET_API_KEY` repository
secret.
