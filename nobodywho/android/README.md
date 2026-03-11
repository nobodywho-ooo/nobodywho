# NobodyWho Android SDK

On-device LLM inference for Android, powered by llama.cpp and UniFFI.

## Requirements

- Android API 24+
- arm64-v8a device (modern Android phones)

## Add to your project

### Via JitPack

```kotlin
// settings.gradle.kts
dependencyResolutionManagement {
    repositories {
        maven { url = uri("https://jitpack.io") }
    }
}

// app/build.gradle.kts
dependencies {
    implementation("com.github.nobodywho-ooo:nobodywho:TAG")
}
```

Replace `TAG` with a release tag from the [releases page](https://github.com/nobodywho-ooo/nobodywho/releases).

### Via local AAR

Build the AAR yourself (see [Building from source](#building-from-source)) then:

```kotlin
// app/build.gradle.kts
dependencies {
    implementation(files("libs/nobodywho-release.aar"))
    implementation("net.java.dev.jna:jna:5.14.0@aar")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.8.1")
}
```

## Models

Any llama.cpp-compatible GGUF model works. Recommended:

| Role | Model | Size |
|------|-------|------|
| Chat | [Qwen3-0.6B-Q4_K_M](https://huggingface.co/Qwen/Qwen3-0.6B-GGUF) | ~400 MB |
| Embedder | [nomic-embed-text-v1.5.Q8_0](https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF) | ~270 MB |
| Reranker | Any cross-encoder GGUF compatible with llama.cpp | varies |

### Loading models

Models must be accessible as a file path on the device. Two common approaches:

**Development — push via ADB:**
```bash
adb push model.gguf     /data/local/tmp/model.gguf
adb push embedder.gguf  /data/local/tmp/embedder.gguf
```

**Production — download at runtime:**
```kotlin
suspend fun downloadModel(context: Context, url: String, filename: String): String {
    val dest = File(context.filesDir, filename)
    if (!dest.exists()) {
        withContext(Dispatchers.IO) {
            URL(url).openStream().use { input ->
                dest.outputStream().use { input.copyTo(it) }
            }
        }
    }
    return dest.absolutePath
}
```

Pass the returned path to `NobodyWhoModel`. The model is loaded once and can be shared across `Chat` and RAG components.

## Quick Start

```kotlin
import com.nobodywho.Chat
import com.nobodywho.NobodyWhoModel

val model = NobodyWhoModel(path = "/data/local/tmp/model.gguf", useGpu = true)
val chat  = Chat(model = model)

// Blocking (call from a coroutine / background thread)
val response = chat.ask("What is the capital of France?")

// Streaming
chat.askFlow("Explain quantum computing").collect { token ->
    print(token)
}
```

## API

### Chat

```kotlin
val model = NobodyWhoModel(path = "/data/local/tmp/model.gguf", useGpu = true)

val chat = Chat(
    model = model,
    config = ChatConfig(
        systemPrompt = "You are a helpful assistant.",
        contextSize = 4096,
        allowThinking = false   // disable <think> tokens (e.g. for Qwen3)
    )
)

val response = chat.ask("What is machine learning?")
```

### Embeddings

```kotlin
val embedder = LlamaCppEmbedder(
    modelPath = "/data/local/tmp/embedder.gguf",
    useGpu = true,
    contextSize = 2048
)

val vector  = embedder.embed("What is RAG?")           // FloatArray
val vectors = embedder.embedBatch(listOf("A", "B"))    // List<FloatArray>
```

### RAG Pipeline

```kotlin
val model       = NobodyWhoModel(path = "chat.gguf", useGpu = true)
val embedder    = LlamaCppEmbedder(modelPath = "embedder.gguf", useGpu = true)
val vectorStore = SQLiteVectorStore(context = applicationContext)
val memory      = HybridSemanticMemory(embedder = embedder, vectorStore = vectorStore)

// Index documents
memory.saveDocument(id = "doc_0", text = "...", metadata = mapOf("source" to "file.pdf"))

// Search and answer
val results = memory.search(query = "your question", topK = 3)
val prompt  = PromptTemplate.default.build(query = "your question", documents = results)
val answer  = Chat(model = model).ask(prompt)
```

### Two-stage retrieval with reranking

```kotlin
val reranker = LlamaCppReranker(modelPath = "reranker.gguf", useGpu = true)
val memory   = HybridSemanticMemory(
    embedder    = embedder,
    vectorStore = vectorStore,
    reranker    = reranker
)

// Fetches 10 candidates, reranks to top 3
val results = memory.search(query = "your question", topK = 3, rerankCandidates = 10)
```

### Vector Stores

| | `InMemoryVectorStore` | `SQLiteVectorStore` |
|--|-----------------------|---------------------|
| Persistence | None | SQLite on disk |
| Max practical size | ~20k docs | ~100k docs |

```kotlin
val inMemory = InMemoryVectorStore()

val sqlite = SQLiteVectorStore(
    context = applicationContext,
    databaseName = "vectors.db"   // stored in app's database directory
)
```

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

## Building from source

### Prerequisites

- Android NDK 27+
- Rust toolchain
- `cargo-ndk`: `cargo install cargo-ndk`

### 1. Generate Kotlin bindings

```bash
cd android
./scripts/generate_bindings.sh
```

### 2. Build the AAR

```bash
export ANDROID_NDK=/path/to/ndk   # e.g. ~/Library/Android/sdk/ndk/27.2.12479018
./scripts/build_aar.sh
```

Output: `library/build/outputs/aar/library-release.aar`

## Architecture

```
android/
├── library/                     # NobodyWho Kotlin SDK (publishable AAR)
│   └── src/main/kotlin/com/nobodywho/
│       ├── Protocols.kt         # EmbedderAgent, VectorStore, Reranker, LanguageModel
│       ├── LlamaCppAgents.kt    # NobodyWhoModel, Chat, LlamaCppEmbedder, LlamaCppReranker
│       ├── VectorStores.kt      # InMemoryVectorStore, SQLiteVectorStore
│       ├── SemanticMemory.kt    # SemanticMemory, HybridSemanticMemory
│       └── Chains.kt            # Chain, PromptTemplate
├── example/                     # Jetpack Compose example app (Chat + RAG tabs)
├── bindgen/                     # uniffi-bindgen binary
└── scripts/
    ├── generate_bindings.sh     # Generate Kotlin bindings from nobodywho.udl
    └── build_aar.sh             # Cross-compile and package the AAR
```
