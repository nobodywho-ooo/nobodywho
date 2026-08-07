package ai.nobodywho

import java.io.Closeable
import uniffi.nobodywho.RustVoiceActivityDetection as InternalVoiceActivityDetection

/**
 * Voice activity detection from live, streaming audio, backed by Silero VAD.
 *
 * Feed each newest chunk to [push] as it arrives — [VoiceActivityDetection] buffers the current
 * turn internally, seeded with a small pre-roll so the confirmed speech isn't
 * clipped at the start. Once [push] returns [VoiceActivityDetectionEvent.SPEECH_ENDED], call
 * [finish] to get that turn's audio and reset for the next one.
 *
 * ```kotlin
 * val vad = VoiceActivityDetection(sampleRate = 16000u)
 * val event = vad.push(chunk)
 * if (event == VoiceActivityDetectionEvent.SPEECH_ENDED) {
 *     val audio = vad.finish()
 * }
 * ```
 */
class VoiceActivityDetection(
    sampleRate: UInt,
    source: String? = null,
    threshold: Float? = null,
    minSilenceDurationMs: UInt? = null,
    minSpeechDurationMs: UInt? = null,
    prerollDurationMs: UInt? = null,
) : Closeable {
    private val inner: InternalVoiceActivityDetection = InternalVoiceActivityDetection(
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
     * [VoiceActivityDetection] tracks the current turn internally). Returns a [VoiceActivityDetectionEvent] if this
     * call crossed a confirmed speech/silence boundary.
     */
    fun push(chunk: List<Short>): VoiceActivityDetectionEvent? = inner.push(chunk)

    /**
     * Return the current turn's captured audio (from the confirmed
     * [VoiceActivityDetectionEvent.SPEECH_STARTED], including a small pre-roll, through to
     * [VoiceActivityDetectionEvent.SPEECH_ENDED]) and reset internal state for the next turn.
     * Empty if speech was never confirmed.
     */
    fun finish(): List<Short> = inner.finish()

    /**
     * Detect every speech segment in a complete audio buffer, returning
     * each segment's audio (with a short pre-roll) in order. Unlike [push],
     * correctly finds every segment regardless of buffer size — use this
     * for offline/batch processing instead of live streaming.
     *
     * ```kotlin
     * for (audio in vad.segment(fullRecording)) {
     *     transcribe(audio)
     * }
     * ```
     */
    fun segment(samples: List<Short>): List<List<Short>> = inner.segment(samples)

    /** Free the underlying Rust resources. */
    fun destroy() = inner.destroy()
    override fun close() { destroy() }
}
