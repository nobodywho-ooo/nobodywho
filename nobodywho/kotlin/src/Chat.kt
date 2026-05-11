package com.nobodywho

import java.io.Closeable
import uniffi.nobodywho.RustChat as InternalRustChat
import uniffi.nobodywho.SamplerConfig
import uniffi.nobodywho.Message

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
    sampler: SamplerConfig? = null
) : Closeable {
    private val inner: InternalRustChat = InternalRustChat(
        model.inner,
        systemPrompt,
        contextSize,
        templateVariables,
        tools?.map { it.inner },
        sampler
    )

    companion object {
        /** Create a chat session directly from a model path. */
        suspend fun fromPath(
            modelPath: String,
            useGpu: Boolean = true,
            projectionModelPath: String? = null,
            systemPrompt: String? = null,
            contextSize: UInt = 4096u,
            templateVariables: Map<String, Boolean>? = null,
            tools: List<Tool>? = null,
            sampler: SamplerConfig? = null,
            onDownloadProgress: ((downloaded: ULong, total: ULong) -> Unit)? = null
        ): Chat {
            val model = Model.load(modelPath, useGpu, projectionModelPath, onDownloadProgress)
            return Chat(model, systemPrompt, contextSize, templateVariables, tools, sampler)
        }
    }

    /** Send a text message and get a token stream for the response. */
    fun ask(message: String) = TokenStream(inner.ask(message))

    /** Send a multimodal prompt and get a token stream. */
    fun ask(prompt: Prompt) = TokenStream(inner.askWithPrompt(prompt.parts))

    fun stopGeneration() = inner.stopGeneration()
    suspend fun resetContext(systemPrompt: String? = null, tools: List<Tool>? = null) =
        inner.resetContext(systemPrompt, tools?.map { it.inner })
    suspend fun resetHistory() = inner.resetHistory()
    suspend fun getChatHistory(): List<Message> = inner.getChatHistory()
    suspend fun setChatHistory(messages: List<Message>) = inner.setChatHistory(messages)
    suspend fun getSystemPrompt(): String? = inner.getSystemPrompt()
    suspend fun setSystemPrompt(systemPrompt: String?) = inner.setSystemPrompt(systemPrompt)
    suspend fun setTools(tools: List<Tool>) = inner.setTools(tools.map { it.inner })
    suspend fun setTemplateVariable(name: String, value: Boolean) = inner.setTemplateVariable(name, value)
    suspend fun getTemplateVariables(): Map<String, Boolean> = inner.getTemplateVariables()
    suspend fun setSamplerConfig(sampler: SamplerConfig) = inner.setSamplerConfig(sampler)
    suspend fun getSamplerConfigJson(): String = inner.getSamplerConfigJson()

    /** Free the underlying Rust resources. */
    fun destroy() = inner.destroy()
    override fun close() { destroy() }
}
