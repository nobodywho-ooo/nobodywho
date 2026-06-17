package ai.nobodywho

import uniffi.nobodywho.PromptPart

/**
 * A multimodal prompt composed of text, image, and audio parts.
 *
 * `Image` / `Audio` reference files on disk. `ImageBytes` accepts encoded
 * image bytes already in memory (PNG/JPEG/etc.). `AudioPcm` accepts 16-bit
 * PCM samples + sample rate (typically 16 kHz, the default).
 *
 * ```kotlin
 * val prompt = Prompt(
 *     Prompt.Text("What do you see?"),
 *     Prompt.Image("./photo.png"),
 *     Prompt.ImageBytes(pngBytes),
 *     Prompt.AudioPcm(pcmSamples, sampleRate = 16000u),
 * )
 * val stream = chat.ask(prompt)
 * ```
 */
class Prompt(vararg parts: Part) {
    internal val parts: List<PromptPart> = parts.map { it.inner }

    class Part internal constructor(internal val inner: PromptPart)

    companion object {
        fun Text(content: String) = Part(PromptPart.Text(content))

        /** Image part from a file-system path. */
        fun Image(path: String) = Part(PromptPart.Image(path))

        /** Image part from in-memory encoded bytes (PNG, JPEG, etc.). */
        fun ImageBytes(data: ByteArray) = Part(PromptPart.ImageBytes(data))

        /** Audio part from a file-system path (WAV/MP3/FLAC). */
        fun Audio(path: String) = Part(PromptPart.Audio(path))

        /**
         * Audio part from in-memory 16-bit PCM samples + sample rate. Every
         * current audio-capable multimodal LLM expects **16 kHz** — resample
         * before passing in. Throws at `ask()` time if the rate doesn't match.
         */
        fun AudioPcm(samples: ShortArray, sampleRate: UInt = 16000u) =
            Part(PromptPart.AudioPcm(samples.toList(), sampleRate))
    }
}
