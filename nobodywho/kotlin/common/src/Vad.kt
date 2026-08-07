package ai.nobodywho

import java.io.Closeable
import uniffi.nobodywho.RustVoiceActivityDetection as InternalVoiceActivityDetection
import uniffi.nobodywho.loadVoiceActivityDetection

/**
 * Voice activity detection from live, streaming audio, backed by Silero VAD.
 *
 * Feed each newest chunk to [push] as it arrives — [VoiceActivityDetection] buffers the current
 * turn internally, seeded with a small pre-roll so the confirmed speech isn't
 * clipped at the start. Once [push] returns [VoiceActivityDetectionEvent.SPEECH_ENDED], call
 * [finish] to get that turn's audio and reset for the next one.
 *
 * ```kotlin
 * val vad = VoiceActivityDetection.load(sampleRate = 16000u)
 * if (vad.push(chunk) == VoiceActivityDetectionEvent.SPEECH_ENDED) {
 *     val audio = vad.finish()
 * }
 * ```
 */
class VoiceActivityDetection internal constructor(
    private val inner: InternalVoiceActivityDetection
) : Closeable {
    companion object {
        /**
         * Load a voice activity detector.
         *
         * @param sampleRate Rate of the audio you'll pass to [push]. Anything other than 16kHz
         *   is resampled internally.
         * @param source HuggingFace repo (`hf://owner/repo`) or local directory for the Silero
         *   VAD ONNX model. Pass `null` to use the default (`hf://onnx-community/silero-vad`).
         */
        suspend fun load(
            sampleRate: UInt,
            source: String? = null,
            threshold: Float? = null,
            minSilenceDurationMs: UInt? = null,
            minSpeechDurationMs: UInt? = null,
            prerollDurationMs: UInt? = null,
        ): VoiceActivityDetection {
            return VoiceActivityDetection(
                loadVoiceActivityDetection(
                    source = source,
                    sampleRate = sampleRate,
                    threshold = threshold,
                    minSilenceDurationMs = minSilenceDurationMs,
                    minSpeechDurationMs = minSpeechDurationMs,
                    prerollDurationMs = prerollDurationMs,
                    device = null
                )
            )
        }
    }

    /**
     * Feed the newest chunk of audio (not the whole accumulated buffer —
     * [VoiceActivityDetection] tracks the current turn internally). Always
     * returns the current confirmed state: [VoiceActivityDetectionEvent.SPEECH]/[VoiceActivityDetectionEvent.SILENCE]
     * if unchanged since the last call, or [VoiceActivityDetectionEvent.SPEECH_STARTED]/[VoiceActivityDetectionEvent.SPEECH_ENDED]
     * on the call that confirmed the transition.
     */
    fun push(chunk: List<Short>): VoiceActivityDetectionEvent = inner.push(chunk)

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
