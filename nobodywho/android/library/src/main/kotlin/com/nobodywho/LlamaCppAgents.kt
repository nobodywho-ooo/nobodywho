@file:JvmName("NobodyWho")

package com.nobodywho

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
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
 *
 * Must be closed when no longer needed to release native memory:
 * ```kotlin
 * val model = loadNobodyWhoModel("/data/local/tmp/model.gguf")
 * model.use {
 *     val chat = Chat(model = it)
 *     chat.ask("Hello")
 * }
 * ```
 */
class NobodyWhoModel internal constructor(internal val ffi: uniffi.nobodywho.Model) : AutoCloseable {
    override fun close() = ffi.close()
}

/** Loads model weights from a `.gguf` file. Call this once and share the result. */
@JvmOverloads
fun loadNobodyWhoModel(path: String, useGpu: Boolean = true): NobodyWhoModel =
    NobodyWhoModel(ffiLoadModel(path = path, useGpu = useGpu))

// MARK: - Chat

/** Configuration for a chat session. */
data class ChatConfig @JvmOverloads constructor(
    val contextSize: Int = 4096,
    val systemPrompt: String? = null,
    val allowThinking: Boolean = true,
    /** Maximum ms to wait for a response. Null means no timeout. */
    val timeoutMs: Long? = null
)

private fun ChatConfig.toFFI() = FFIChatConfig(
    contextSize = contextSize.toUInt(),
    systemPrompt = systemPrompt,
    allowThinking = allowThinking
)

/**
 * On-device LLM chat backed by a llama.cpp model.
 *
 * Must be closed when no longer needed to release native memory.
 *
 * ```kotlin
 * // Option A — load model and create chat in one step:
 * Chat(modelPath = "/data/local/tmp/model.gguf").use { chat ->
 *     val response = chat.ask("Hello")
 * }
 *
 * // Option B — share pre-loaded weights across multiple Chat instances:
 * val model = loadNobodyWhoModel("/data/local/tmp/model.gguf")
 * val chatA = Chat(model = model, config = ChatConfig(systemPrompt = "You are a pirate."))
 * val chatB = Chat(model = model)
 * ```
 */
class Chat private constructor(private val inner: FFIChat, private val config: ChatConfig) : AutoCloseable {

    /** Loads model weights from [modelPath] and creates a chat session. */
    @JvmOverloads
    constructor(
        modelPath: String,
        useGpu: Boolean = true,
        config: ChatConfig = ChatConfig()
    ) : this(FFIChat(model = ffiLoadModel(path = modelPath, useGpu = useGpu), config = config.toFFI()), config)

    /** Creates a chat session from already-loaded [model] weights. No file I/O occurs. */
    @JvmOverloads
    constructor(
        model: NobodyWhoModel,
        config: ChatConfig = ChatConfig()
    ) : this(FFIChat(model = model.ffi, config = config.toFFI()), config)

    /**
     * Generates a response and returns the full text when complete.
     * Throws [kotlinx.coroutines.TimeoutCancellationException] if [ChatConfig.timeoutMs] is set and exceeded.
     */
    suspend fun ask(prompt: String): String = withContext(Dispatchers.IO) {
        val timeoutMs = config.timeoutMs
        if (timeoutMs != null) {
            withTimeoutOrNull(timeoutMs) { inner.ask(prompt = prompt).completed() }
                ?: error("Inference timed out after ${timeoutMs}ms")
        } else {
            inner.ask(prompt = prompt).completed()
        }
    }

    /**
     * Emits tokens as they are generated.
     * The token stream is always closed — even if the Flow is cancelled mid-generation.
     */
    fun askFlow(prompt: String): Flow<String> = flow {
        val stream = withContext(Dispatchers.IO) { inner.ask(prompt = prompt) }
        try {
            while (true) {
                val token = withContext(Dispatchers.IO) { stream.nextToken() } ?: break
                emit(token)
            }
        } finally {
            withContext(NonCancellable + Dispatchers.IO) { stream.close() }
        }
    }

    override fun close() = inner.close()
}

// MARK: - LlamaCppEmbedder

/** `EmbedderAgent` backed by a llama.cpp embedding model. */
class LlamaCppEmbedder @JvmOverloads constructor(
    modelPath: String,
    useGpu: Boolean = true,
    contextSize: Int = 512
) : EmbedderAgent, AutoCloseable {
    private val embedder = loadEmbedder(
        path = modelPath,
        useGpu = useGpu,
        contextSize = contextSize.toUInt()
    )

    override fun embed(text: String): FloatArray =
        embedder.embed(text = text).toFloatArray()

    override fun embedBatch(texts: List<String>): List<FloatArray> =
        embedder.embedBatch(texts = texts).map { it.toFloatArray() }

    override fun close() = embedder.close()
}

// MARK: - LlamaCppReranker

/** `Reranker` backed by a llama.cpp cross-encoder model. */
class LlamaCppReranker @JvmOverloads constructor(
    modelPath: String,
    useGpu: Boolean = true,
    contextSize: Int = 4096
) : Reranker, AutoCloseable {
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

    override fun close() = crossEncoder.close()
}

// MARK: - LlamaCppLanguageModel

/** `LanguageModel` backed by a llama.cpp chat model. */
class LlamaCppLanguageModel @JvmOverloads constructor(
    modelPath: String,
    useGpu: Boolean = true,
    config: ChatConfig = ChatConfig()
) : LanguageModel, AutoCloseable {
    private val chat = Chat(modelPath = modelPath, useGpu = useGpu, config = config)

    override suspend fun generate(prompt: String): String = chat.ask(prompt)

    override fun generateFlow(prompt: String): Flow<String> = chat.askFlow(prompt)

    override fun close() = chat.close()
}
