package com.nobodywho

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.withContext
import uniffi.nobodywho.ChatConfig as FFIChatConfig
import uniffi.nobodywho.Chat as FFIChat
import uniffi.nobodywho.loadModel as ffiLoadModel
import uniffi.nobodywho.loadEmbedder
import uniffi.nobodywho.loadCrossEncoder

// MARK: - NobodyWhoModel

/**
 * An opaque handle to loaded model weights.
 *
 * Load once via [loadNobodyWhoModel] and share between [Chat] instances so the weights
 * are not duplicated in memory. Each [Chat] created from the same model gets its own
 * independent context and conversation history.
 */
class NobodyWhoModel internal constructor(internal val ffi: uniffi.nobodywho.Model)

/** Loads model weights from a `.gguf` file. Call this once and share the result. */
fun loadNobodyWhoModel(path: String, useGpu: Boolean = true): NobodyWhoModel =
    NobodyWhoModel(ffiLoadModel(path = path, useGpu = useGpu))

// MARK: - Chat

/** Configuration for a chat session. */
data class ChatConfig(
    val contextSize: Int = 4096,
    val systemPrompt: String? = null,
    val allowThinking: Boolean = true
)

private fun ChatConfig.toFFI() = FFIChatConfig(
    contextSize = contextSize.toUInt(),
    systemPrompt = systemPrompt,
    allowThinking = allowThinking
)

/**
 * On-device LLM chat backed by a llama.cpp model.
 *
 * ```kotlin
 * // Option A — load model and create chat in one step:
 * val chat = Chat(modelPath = "/data/local/tmp/model.gguf")
 *
 * // Option B — share pre-loaded weights across multiple Chat instances:
 * val model = loadNobodyWhoModel("/data/local/tmp/model.gguf")
 * val chatA = Chat(model = model, config = ChatConfig(systemPrompt = "You are a pirate."))
 * val chatB = Chat(model = model)
 * ```
 */
class Chat private constructor(private val inner: FFIChat) {

    /** Loads model weights from [modelPath] and creates a chat session. */
    constructor(
        modelPath: String,
        useGpu: Boolean = true,
        config: ChatConfig = ChatConfig()
    ) : this(FFIChat(model = ffiLoadModel(path = modelPath, useGpu = useGpu), config = config.toFFI()))

    /** Creates a chat session from already-loaded [model] weights. No file I/O occurs. */
    constructor(
        model: NobodyWhoModel,
        config: ChatConfig = ChatConfig()
    ) : this(FFIChat(model = model.ffi, config = config.toFFI()))

    /** Generates a response and returns the full text when complete. */
    suspend fun ask(prompt: String): String = withContext(Dispatchers.IO) {
        inner.ask(prompt = prompt).completed()
    }

    /** Emits tokens as they are generated. */
    fun askFlow(prompt: String): Flow<String> = flow {
        val stream = inner.ask(prompt = prompt)
        while (true) {
            val token = withContext(Dispatchers.IO) { stream.nextToken() } ?: break
            emit(token)
        }
    }
}

// MARK: - LlamaCppEmbedder

/** `EmbedderAgent` backed by a llama.cpp embedding model. */
class LlamaCppEmbedder(
    modelPath: String,
    useGpu: Boolean = true,
    contextSize: Int = 512
) : EmbedderAgent {
    private val embedder = loadEmbedder(
        path = modelPath,
        useGpu = useGpu,
        contextSize = contextSize.toUInt()
    )

    override fun embed(text: String): FloatArray =
        embedder.embed(text = text).toFloatArray()

    override fun embedBatch(texts: List<String>): List<FloatArray> =
        embedder.embedBatch(texts = texts).map { it.toFloatArray() }
}

// MARK: - LlamaCppReranker

/** `Reranker` backed by a llama.cpp cross-encoder model. */
class LlamaCppReranker(
    modelPath: String,
    useGpu: Boolean = true,
    contextSize: Int = 4096
) : Reranker {
    private val crossEncoder = loadCrossEncoder(
        path = modelPath,
        useGpu = useGpu,
        contextSize = contextSize.toUInt()
    )

    override fun rank(query: String, documents: List<String>): FloatArray =
        crossEncoder.rank(query = query, documents = documents).toFloatArray()

    override fun rankAndSort(query: String, documents: List<String>): List<RankedResult> =
        crossEncoder.rankAndSort(query = query, documents = documents)
            .map { RankedResult(content = it.content, score = it.score) }
}

// MARK: - LlamaCppLanguageModel

/** `LanguageModel` backed by a llama.cpp chat model. */
class LlamaCppLanguageModel(
    modelPath: String,
    useGpu: Boolean = true,
    config: ChatConfig = ChatConfig()
) : LanguageModel {
    private val chat = Chat(modelPath = modelPath, useGpu = useGpu, config = config)

    override suspend fun generate(prompt: String): String = chat.ask(prompt)

    override fun generateFlow(prompt: String): Flow<String> = chat.askFlow(prompt)
}
