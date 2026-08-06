package ai.nobodywho

import java.io.Closeable
import uniffi.nobodywho.RustVad as InternalVad

/**
 * Voice activity detection from live, streaming audio, backed by Silero VAD.
 *
 * Feed each newest chunk to [push] as it arrives — [Vad] buffers the current
 * turn internally, seeded with a small pre-roll so the confirmed speech isn't
 * clipped at the start. Once [push] returns [VadEvent.SPEECH_ENDED], call
 * [finish] to get that turn's audio and reset for the next one.
 *
 * ```kotlin
 * val vad = Vad(sampleRate = 16000u)
 * val event = vad.push(chunk)
 * if (event == VadEvent.SPEECH_ENDED) {
 *     val audio = vad.finish()
 * }
 * ```
 */
class Vad(
    sampleRate: UInt,
    source: String? = null,
    threshold: Float? = null,
    minSilenceDurationMs: UInt? = null,
    minSpeechDurationMs: UInt? = null,
    prerollDurationMs: UInt? = null,
) : Closeable {
    private val inner: InternalVad = InternalVad(
        source = source,
        sampleRate = sampleRate,
        threshold = threshold,
        minSilenceDurationMs = minSilenceDurationMs,
        minSpeechDurationMs = minSpeechDurationMs,
        prerollDurationMs = prerollDurationMs,
        device = null
    )

    /**
     * Feed the newest chunk of audio (not the whole accumulated buffer —
     * [Vad] tracks the current turn internally). Returns a [VadEvent] if this
     * call crossed a confirmed speech/silence boundary.
     */
    fun push(chunk: List<Short>): VadEvent? = inner.push(chunk)

    /**
     * Return the current turn's captured audio (from the confirmed
     * [VadEvent.SPEECH_STARTED], including a small pre-roll, through to
     * [VadEvent.SPEECH_ENDED]) and reset internal state for the next turn.
     * Empty if speech was never confirmed.
     */
    fun finish(): List<Short> = inner.finish()

    /** Free the underlying Rust resources. */
    fun destroy() = inner.destroy()
    override fun close() { destroy() }
}
