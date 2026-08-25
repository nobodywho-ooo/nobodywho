package ai.nobodywho

// Re-export uniffi types that are part of the public API so consumers
// only need to import from ai.nobodywho.
typealias SamplerConfig = uniffi.nobodywho.SamplerConfig
typealias ToolCall = uniffi.nobodywho.ToolCall
typealias CachedModel = uniffi.nobodywho.CachedModel
typealias VoiceActivityDetectionEvent = uniffi.nobodywho.VoiceActivityDetectionEvent
typealias ContentPart = uniffi.nobodywho.ContentPart
typealias MessageContent = uniffi.nobodywho.MessageContent

// Fully qualified on the right-hand side: these functions share the name of the
// type, so an unqualified `MessageContent` here resolves to the function.

/** Content holding a single run of text. */
fun MessageContent(text: String): MessageContent =
    uniffi.nobodywho.MessageContent.Text(text)

/** Content holding interleaved text and media. */
fun MessageContent(vararg parts: ContentPart): MessageContent =
    uniffi.nobodywho.MessageContent.Parts(parts.toList())

/**
 * This content as text.
 *
 * Media parts are left out, so for content that interleaves text and media this
 * is only the text around it — match on the content itself when the media
 * matters.
 */
val MessageContent.text: String
    get() = when (this) {
        is uniffi.nobodywho.MessageContent.Text -> text
        is uniffi.nobodywho.MessageContent.Json -> json
        is uniffi.nobodywho.MessageContent.Parts ->
            parts.filterIsInstance<uniffi.nobodywho.ContentPart.Text>()
                .joinToString("") { it.text }
    }

/**
 * A message in the chat history.
 *
 * - [User] — a user message, whose content may interleave text and media
 * - [Assistant] — an assistant response, optionally with tool calls
 * - [System] — a system prompt
 * - [Tool] — the result returned by a tool invocation
 *
 * Each role takes either a [MessageContent] or, for the common case, a plain
 * string.
 */
sealed class Message {
    data class User(val content: MessageContent) : Message() {
        constructor(text: String) : this(MessageContent(text))
        constructor(vararg parts: ContentPart) : this(MessageContent(*parts))
    }

    data class Assistant(
        val content: MessageContent,
        val toolCalls: List<ToolCall>? = null,
    ) : Message() {
        constructor(text: String, toolCalls: List<ToolCall>? = null) :
            this(MessageContent(text), toolCalls)
    }

    data class System(val content: MessageContent) : Message() {
        constructor(text: String) : this(MessageContent(text))
    }

    data class Tool(val name: String, val content: MessageContent) : Message() {
        constructor(name: String, text: String) : this(name, MessageContent(text))
    }

    companion object {
        internal fun fromUniFFI(msg: uniffi.nobodywho.Message): Message = when (msg) {
            is uniffi.nobodywho.Message.User -> User(msg.content)
            is uniffi.nobodywho.Message.Assistant -> Assistant(msg.content, msg.toolCalls)
            is uniffi.nobodywho.Message.System -> System(msg.content)
            is uniffi.nobodywho.Message.Tool -> Tool(msg.name, msg.content)
        }

        internal fun toUniFFI(msg: Message): uniffi.nobodywho.Message = when (msg) {
            is User -> uniffi.nobodywho.Message.User(msg.content)
            is Assistant -> uniffi.nobodywho.Message.Assistant(msg.content, msg.toolCalls)
            is System -> uniffi.nobodywho.Message.System(msg.content)
            is Tool -> uniffi.nobodywho.Message.Tool(msg.name, msg.content)
        }
    }
}
