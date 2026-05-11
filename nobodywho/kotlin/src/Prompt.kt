package com.nobodywho

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
 * ```
 */
class Prompt(vararg parts: Part) {
    internal val parts: List<PromptPart> = parts.map { it.inner }

    class Part internal constructor(internal val inner: PromptPart)

    companion object {
        fun Text(content: String) = Part(PromptPart.Text(content))
        fun Image(path: String) = Part(PromptPart.Image(path))
        fun Audio(path: String) = Part(PromptPart.Audio(path))
    }
}
