package ai.nobodywho

import java.io.Closeable
import uniffi.nobodywho.ChatStats
import uniffi.nobodywho.MtpConfig
import uniffi.nobodywho.RustChat as InternalRustChat
import uniffi.nobodywho.SamplerConfig

/**
 * A chat session for local LLM inference.
 *
 * ```kotlin
 * val model = Model.load("model.gguf")
 * val chat = Chat(
 *     model = model,
 *     systemPrompt = "You are a helpful assistant."
 * )
 * chat.ask("Hello!").asFlow().collect { token ->
 *     print(token)
 * }
 * ```
 */
class Chat(
    model: Model,
    systemPrompt: String? = null,
    contextSize: UInt = 4096u,
    templateVariables: Map<String, Boolean>? = null,
    tools: List<Tool>? = null,
    sampler: SamplerConfig? = null,
    /**
     * MTP speculative decoding config. Pass an [MtpConfig] to enable MTP
     * (e.g. `MtpConfig()` for defaults); leave null to disable. Requires the
     * [Model] to have been loaded with a compatible `draftModelPath`. Adds
     * around 5% to VRAM usage.
     */
    mtp: MtpConfig? = null
) : Closeable {
    private val inner: InternalRustChat = InternalRustChat(
        model.inner,
        systemPrompt,
        contextSize,
        templateVariables,
        tools?.map { it.inner },
        sampler,
        mtp
    )

    companion object {
        /** Create a chat session directly from a model path. */
        suspend fun fromPath(
            modelPath: String,
            useGpu: Boolean = true,
            projectionModelPath: String? = null,
            draftModelPath: String? = null,
            systemPrompt: String? = null,
            contextSize: UInt = 4096u,
            templateVariables: Map<String, Boolean>? = null,
            tools: List<Tool>? = null,
            sampler: SamplerConfig? = null,
            mtp: MtpConfig? = null,
            onDownloadProgress: ((downloaded: ULong, total: ULong) -> Unit)? = null
        ): Chat {
            val model = Model.load(modelPath, useGpu, projectionModelPath, draftModelPath, onDownloadProgress)
            return Chat(model, systemPrompt, contextSize, templateVariables, tools, sampler, mtp)
        }
    }

    /** Send a text message and get a token stream for the response. */
    fun ask(message: String) = TokenStream(inner.ask(message))

    /** Send a multimodal prompt and get a token stream. */
    fun ask(prompt: Prompt): TokenStream {
        prompt.jsonString?.let { return TokenStream(inner.askWithJsonPrompt(it)) }
        return TokenStream(inner.askWithPrompt(prompt.parts!!))
    }

    fun stopGeneration() = inner.stopGeneration()
    suspend fun resetContext(systemPrompt: String? = null, tools: List<Tool>? = null) =
        inner.resetContext(systemPrompt, tools?.map { it.inner })
    suspend fun resetHistory() = inner.resetHistory()
    // getChatHistory/setChatHistory convert between our Message wrapper and the
    // uniffi-generated Message. We wrap Message so that consumers only import from
    // ai.nobodywho — the generated uniffi.nobodywho.Message.Tool variant would
    // otherwise clash with the top-level ai.nobodywho.Tool class.
    suspend fun getChatHistory(): List<Message> = inner.getChatHistory().map { Message.fromUniFFI(it) }
    suspend fun setChatHistory(messages: List<Message>) = inner.setChatHistory(messages.map { Message.toUniFFI(it) })
    suspend fun getSystemPrompt(): String? = inner.getSystemPrompt()
    suspend fun setSystemPrompt(systemPrompt: String?) = inner.setSystemPrompt(systemPrompt)
    suspend fun setTools(tools: List<Tool>) = inner.setTools(tools.map { it.inner })
    suspend fun setTemplateVariable(name: String, value: Boolean) = inner.setTemplateVariable(name, value)
    suspend fun getTemplateVariables(): Map<String, Boolean> = inner.getTemplateVariables()
    suspend fun setSamplerConfig(sampler: SamplerConfig) = inner.setSamplerConfig(sampler)
    suspend fun getSamplerConfigJson(): String = inner.getSamplerConfigJson()
    suspend fun getStats(): ChatStats = inner.getStats()

    /**
     * MTP draft acceptance rate for the most recent generation, in `[0.0, 1.0]`.
     * Resets each generation (per-response, not cumulative). `null` when MTP is
     * disabled on this chat or no drafts were proposed in the last generation.
     */
    suspend fun mtpAcceptanceRate(): Float? = inner.mtpAcceptanceRate()
    suspend fun tokenize(message: String): List<Int?> = inner.tokenize(message)

    /** Free the underlying Rust resources. */
    fun destroy() = inner.destroy()
    override fun close() { destroy() }
}
