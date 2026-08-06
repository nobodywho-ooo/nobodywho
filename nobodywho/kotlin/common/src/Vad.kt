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

    /**
     * Run whatever complete Silero frames [chunk] completes through the
     * model and return their raw speech probabilities, in order — no
     * debouncing, no audio buffering. For callers who want their own
     * thresholding instead of [push]'s built-in debounce logic, or who want
     * zero memory overhead beyond fixed model state. Safe to call with any
     * chunk size, from a live mic buffer up to an entire recording at once.
     * If you reuse this [Vad] across unrelated audio sessions, call [finish]
     * in between to clear state so it doesn't leak across sessions.
     */
    fun predict(chunk: List<Short>): List<Float> = inner.predict(chunk)

    /**
     * Detect every speech segment in a complete audio buffer at once,
     * returning each segment's audio (with a small pre-roll lead-in) in
     * order. Unlike [push], this is guaranteed not to drop a transition
     * regardless of buffer size — the right tool for offline/batch
     * processing of a full recording rather than live streaming.
     */
    fun segment(samples: List<Short>): List<List<Short>> = inner.segment(samples)

    /** Free the underlying Rust resources. */
    fun destroy() = inner.destroy()
    override fun close() { destroy() }
}
