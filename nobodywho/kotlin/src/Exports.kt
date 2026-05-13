package ai.nobodywho

// Re-export uniffi types that are part of the public API so consumers
// only need to import from ai.nobodywho.
typealias SamplerConfig = uniffi.nobodywho.SamplerConfig
typealias Asset = uniffi.nobodywho.Asset
typealias ToolCall = uniffi.nobodywho.ToolCall

/**
 * A message in the chat history.
 *
 * - [User] — a user message, optionally with image/audio assets
 * - [Assistant] — an assistant response, optionally with tool calls
 * - [System] — a system prompt
 * - [Tool] — the result returned by a tool invocation
 */
sealed class Message {
    data class User(val content: String, val assets: List<Asset> = emptyList()) : Message()
    data class Assistant(val content: String, val toolCalls: List<ToolCall>? = null) : Message()
    data class System(val content: String) : Message()
    data class Tool(val name: String, val content: String) : Message()

    companion object {
        internal fun fromUniFFI(msg: uniffi.nobodywho.Message): Message = when (msg) {
            is uniffi.nobodywho.Message.User -> User(msg.content, msg.assets)
            is uniffi.nobodywho.Message.Assistant -> Assistant(msg.content, msg.toolCalls)
            is uniffi.nobodywho.Message.System -> System(msg.content)
            is uniffi.nobodywho.Message.Tool -> Tool(msg.name, msg.content)
        }

        internal fun toUniFFI(msg: Message): uniffi.nobodywho.Message = when (msg) {
            is User -> uniffi.nobodywho.Message.User(msg.content, msg.assets)
            is Assistant -> uniffi.nobodywho.Message.Assistant(msg.content, msg.toolCalls)
            is System -> uniffi.nobodywho.Message.System(msg.content)
            is Tool -> uniffi.nobodywho.Message.Tool(msg.name, msg.content)
        }
    }
}
