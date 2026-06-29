package ai.nobodywho

import org.json.JSONArray
import org.json.JSONObject
import uniffi.nobodywho.PromptPart

/**
 * A multimodal prompt composed of text, image, and audio parts.
 *
 * ```kotlin
 * val prompt = Prompt(
 *     Prompt.Text("What do you see?"),
 *     Prompt.Image("./photo.png"),
 * )
 * val stream = chat.ask(prompt)
 *
 * // JSON-structured prompt
 * val jsonPrompt = Prompt.fromJson(mapOf("key" to "value"))
 * val stream = chat.ask(jsonPrompt)
 * ```
 */
class Prompt private constructor(
    internal val parts: List<PromptPart>?,
    internal val jsonString: String?,
) {
    constructor(vararg parts: Part) : this(parts.map { it.inner }, null)

    class Part internal constructor(internal val inner: PromptPart)

    companion object {
        fun Text(content: String) = Part(PromptPart.Text(content))
        fun Image(path: String) = Part(PromptPart.Image(path))
        fun Audio(path: String) = Part(PromptPart.Audio(path))

        /** Create a prompt from any JSON-serializable value (Map, List, String, etc.). */
        fun fromJson(data: Any): Prompt = Prompt(null, toJson(data).toString())

        private fun toJson(value: Any?): Any =
            when (value) {
                null -> JSONObject.NULL
                is Map<*, *> -> JSONObject().apply { value.forEach { (k, v) -> put(k.toString(), toJson(v)) } }
                is List<*> -> JSONArray().apply { value.forEach { put(toJson(it)) } }
                else -> value
            }
    }
}
