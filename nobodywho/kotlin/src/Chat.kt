package com.nobodywho

import uniffi.nobodywho.RustChat as InternalRustChat
import uniffi.nobodywho.Model
import uniffi.nobodywho.SamplerConfig
import uniffi.nobodywho.Message
import uniffi.nobodywho.PromptPart

/**
 * A chat session for local LLM inference.
 *
 * Wraps the internal `RustChat` with an ergonomic API that uses
 * the wrapper [Tool] and [TokenStream] types.
 *
 * ```kotlin
 * val model = loadModel("model.gguf", useGpu = true, imageModelPath = null)
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
) {
    private val inner: InternalRustChat = InternalRustChat(
        model,
        systemPrompt,
        contextSize,
        templateVariables,
        tools?.map { it.inner },
        sampler
    )

    /** Send a text message and get a token stream for the response. */
    fun ask(message: String): TokenStream {
        return TokenStream(inner.ask(message))
    }

    /** Send a multimodal prompt (text + images/audio) and get a token stream. */
    fun askWithPrompt(parts: List<PromptPart>): TokenStream {
        return TokenStream(inner.askWithPrompt(parts))
    }

    /** Stop the current generation. */
    fun stopGeneration() {
        inner.stopGeneration()
    }

    /** Reset the chat context with a new system prompt and tools. */
    suspend fun resetContext(systemPrompt: String? = null, tools: List<Tool>? = null) {
        inner.resetContext(systemPrompt, tools?.map { it.inner })
    }

    /** Reset the chat history, keeping the system prompt and tools. */
    suspend fun resetHistory() {
        inner.resetHistory()
    }

    /** Get the current chat history as a list of messages. */
    suspend fun getChatHistory(): List<Message> {
        return inner.getChatHistory()
    }

    /** Set the chat history from a list of messages. */
    suspend fun setChatHistory(messages: List<Message>) {
        inner.setChatHistory(messages)
    }

    /** Get the current system prompt. */
    suspend fun getSystemPrompt(): String? {
        return inner.getSystemPrompt()
    }

    /** Set the system prompt. */
    suspend fun setSystemPrompt(systemPrompt: String?) {
        inner.setSystemPrompt(systemPrompt)
    }

    /** Set the tools available to the model. */
    suspend fun setTools(tools: List<Tool>) {
        inner.setTools(tools.map { it.inner })
    }

    /** Set a template variable. */
    suspend fun setTemplateVariable(name: String, value: Boolean) {
        inner.setTemplateVariable(name, value)
    }

    /** Get all template variables. */
    suspend fun getTemplateVariables(): Map<String, Boolean> {
        return inner.getTemplateVariables()
    }

    /** Set the sampler configuration. */
    suspend fun setSamplerConfig(sampler: SamplerConfig) {
        inner.setSamplerConfig(sampler)
    }

    /** Get the current sampler configuration as a JSON string. */
    suspend fun getSamplerConfigJson(): String {
        return inner.getSamplerConfigJson()
    }
}
