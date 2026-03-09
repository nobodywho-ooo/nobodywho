# NobodyWho Android SDK

On-device LLM inference for Android, powered by llama.cpp and UniFFI.

## Requirements

- Android API 24+
- Android NDK 25+
- Rust toolchain with Android targets

## Quick Start

```kotlin
import com.nobodywho.Chat

val chat = Chat(modelPath = "/data/local/tmp/model.gguf", useGpu = true)
val response = chat.ask("What is the capital of France?")
```

## Setup

### 1. Generate Kotlin bindings

```bash
cd android
./scripts/generate_bindings.sh
```

This runs `uniffi-bindgen` against the Rust UDL and writes the generated Kotlin to
`library/src/main/kotlin/uniffi/nobodywho/`.

### 2. Build the AAR

```bash
export ANDROID_NDK=/path/to/ndk
./scripts/build_aar.sh
```

The script cross-compiles for `arm64-v8a`, `armeabi-v7a`, `x86_64`, and `x86`,
copies the `.so` files to `library/src/main/jniLibs/`, then runs Gradle to produce
`library/build/outputs/aar/library-release.aar`.

### 3. Add to your app

```kotlin
// settings.gradle.kts
include(":nobodywho-library")
project(":nobodywho-library").projectDir = File("path/to/android/library")

// app/build.gradle.kts
dependencies {
    implementation(project(":nobodywho-library"))
}
```

Or add the published AAR as a file dependency:

```kotlin
dependencies {
    implementation(files("libs/nobodywho-release.aar"))
    implementation("net.java.dev.jna:jna:5.14.0@aar")
}
```

## API

### Chat

```kotlin
val chat = Chat(
    modelPath = "/data/local/tmp/model.gguf",
    useGpu = true,
    config = ChatConfig(contextSize = 4096, systemPrompt = "You are a helpful assistant.")
)

// Blocking (call from a coroutine or background thread)
val response = chat.ask("What is machine learning?")

// Streaming
chat.askFlow("Explain quantum computing").collect { token ->
    print(token)
}
```

### Embeddings

```kotlin
val embedder = LlamaCppEmbedder(
    modelPath = "/data/local/tmp/embedder.gguf",
    useGpu = true,
    contextSize = 2048
)

val vector = embedder.embed("What is RAG?")          // FloatArray (768 dims)
val vectors = embedder.embedBatch(listOf("A", "B"))  // List<FloatArray>
```

### RAG Pipeline

```kotlin
val embedder    = LlamaCppEmbedder(modelPath = "embedder.gguf", useGpu = true)
val vectorStore = SQLiteVectorStore(context = applicationContext)
val memory      = HybridSemanticMemory(embedder = embedder, vectorStore = vectorStore)

// Index documents
memory.saveDocument(id = "doc_0", text = "...", metadata = mapOf("source" to "file.pdf"))

// Search
val results = memory.search(query = "your question", topK = 3)
val prompt  = PromptTemplate.default.build(query = "your question", documents = results)

val chat   = Chat(modelPath = "chat.gguf")
val answer = chat.ask(prompt)
```

### Two-stage retrieval with reranking

```kotlin
val reranker = LlamaCppReranker(modelPath = "reranker.gguf", useGpu = true)
val memory   = HybridSemanticMemory(embedder = embedder, vectorStore = vectorStore, reranker = reranker)

// Fetches 10 candidates, reranks to top 3
val results = memory.search(query = "your question", topK = 3, rerankCandidates = 10)
```

### Vector Stores

| | `InMemoryVectorStore` | `SQLiteVectorStore` |
|--|-----------------------|---------------------|
| Persistence | None | SQLite on disk |
| API level | 24+ | 24+ |
| Max practical size | ~20k docs | ~100k docs |
| iCloud/sync | No | Manual |

```kotlin
val inMemory = InMemoryVectorStore()

val sqlite = SQLiteVectorStore(
    context = applicationContext,
    databaseName = "vectors.db"  // stored in app's database directory
)
```

## Architecture

```
android/
├── library/                     # NobodyWho Kotlin SDK (publishable AAR)
│   └── src/main/kotlin/com/nobodywho/
│       ├── Protocols.kt         # EmbedderAgent, VectorStore, Reranker, LanguageModel
│       ├── LlamaCppAgents.kt    # Chat, LlamaCppEmbedder, LlamaCppReranker
│       ├── VectorStores.kt      # InMemoryVectorStore, SQLiteVectorStore
│       ├── SemanticMemory.kt    # SemanticMemory, HybridSemanticMemory
│       └── Chains.kt            # Chain, PromptTemplate
├── example/                     # Jetpack Compose example app
│   └── src/main/kotlin/com/nobodywho/example/
│       ├── MainActivity.kt      # TabView entry point (Chat + RAG tabs)
│       ├── ChatViewModel.kt
│       ├── ChatScreen.kt
│       ├── RAGViewModel.kt
│       └── RAGScreen.kt
├── bindgen/                     # uniffi-bindgen binary (generates Kotlin from UDL)
└── scripts/
    ├── generate_bindings.sh     # Generate Kotlin bindings from nobodywho.udl
    └── build_aar.sh             # Cross-compile and package the AAR
```

## Models

Place `.gguf` model files where your app can access them. For development, push via ADB:

```bash
adb push model.gguf /data/local/tmp/model.gguf
adb push embedder.gguf /data/local/tmp/embedder.gguf
```

In production, download models to `context.filesDir` at runtime and pass that path to the SDK.

Recommended models:
- **Chat**: `Qwen3-0.6B-Q4_K_M.gguf` (~400 MB) — practical for on-device use
- **Embedder**: `nomic-embed-text-v1.5.Q8_0.gguf` (~270 MB)
- **Reranker**: Any cross-encoder `.gguf` compatible with llama.cpp

## Performance (Pixel 8)

| Operation | Time |
|-----------|------|
| Load chat model | ~500 ms |
| Load embedder | ~300 ms |
| First token | ~500 ms |
| Embed single text | ~50 ms |
| Vector search — 10k docs | ~15 ms |
| Rerank top 10 | ~150 ms |

GPU acceleration via Vulkan is available on supported devices.
