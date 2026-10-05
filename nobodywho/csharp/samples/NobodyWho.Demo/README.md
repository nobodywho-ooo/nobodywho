# NobodyWho .NET demo

A Blazor Server app that shows what the NobodyWho .NET library can do, one page per capability, each
next to the C# it runs:

| Page | Shows |
|------|-------|
| Overview | A streaming answer, every model's status, and the download cache |
| Chat | Multi-turn streaming, system prompt, sampler presets, reasoning ("thinking"), stopping a reply, context usage |
| Tool calling | Three C# lambdas the model calls, with each call's arguments and result as it happens |
| Structured output | JSON-schema output deserialized into a record, and regex-constrained classification |
| Search and rerank | Embedding search, cross-encoder reranking, and an answer grounded in the top results |
| Images | Questions about an uploaded image (needs a vision model; see below) |
| Speech | Text to speech, transcription, and voice activity detection, in one loop with no microphone |

Everything runs inside the web server process. Models given as `hf://` paths download on first use
and are cached; nothing else leaves the machine.

## Run it

The app references the library project, so it needs the native library built first. From
`nobodywho/` (the Cargo workspace):

```bash
cargo build -p nobodywho-uniffi
cd csharp/samples/NobodyWho.Demo
dotnet run
```

Then open the URL it prints. The first visit to each page downloads its model, and the overview page
shows the progress.

### Without a Rust toolchain

Use the dev container instead. From the repository root:

```bash
docker build -t nobodywho-csharp-dev -f nobodywho/csharp/dev.Dockerfile .
docker run --rm -p 5080:5080 -v "$PWD:/src" -w /src/nobodywho nobodywho-csharp-dev bash -c \
  "cargo build -p nobodywho-uniffi && dotnet run --project csharp/samples/NobodyWho.Demo --urls http://0.0.0.0:5080"
```

Then open <http://localhost:5080>. The first build compiles llama.cpp and takes a few minutes.

## Models

Set them under `NobodyWho` in `appsettings.json`, or override any one with an environment variable
such as `NobodyWho__ChatModel=/path/to/model.gguf`.

| Setting | Default | Used by |
|---------|---------|---------|
| `ChatModel` | Qwen3 0.6B | Overview, Chat, Tool calling, Structured output, answers on Search |
| `EmbeddingModel` | bge-small-en-v1.5 | Search |
| `RerankerModel` | bge-reranker-v2-m3 | Rerank on Search |
| `VisionModel`, `VisionProjection` | Not set | Images |
| `TextToSpeech`, `TextToSpeechVoice` | Kokoro, `bf_emma` | Speech |
| `SpeechToText` | Whisper base | Speech |
| `VoiceActivityDetection` | Silero VAD | Speech |
| `UseGpu` | `true` | All GGUF models; falls back to the CPU without a GPU |

The Images page needs a multimodal model and its projection file, for example Gemma 4 E2B:

```json
"VisionModel": "hf://bartowski/google_gemma-4-E2B-it-GGUF/google_gemma-4-E2B-it-Q4_K_M.gguf",
"VisionProjection": "hf://bartowski/google_gemma-4-E2B-it-GGUF/mmproj-google_gemma-4-E2B-it-f16.gguf"
```

## How it's put together

- `Services/ModelHub.cs` loads each model once for the whole app, on first use, and reports
  download progress. Models are large; chats are cheap, so each page makes its own `Chat` on a
  shared `Model`.
- `Components/Shared/ModelGate.razor` shows a model's loading state and renders the demo once it's
  ready.
- `Components/Shared/DemoPage.cs` stops generation and frees a page's chats when the visitor leaves.
- Prerendering is off: prerendered buttons do nothing until the live connection starts, and a
  prerender would build a throwaway `Chat` on every visit.
