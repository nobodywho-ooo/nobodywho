package ai.nobodywho

import java.io.Closeable
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import uniffi.nobodywho.RustSpeechToText as InternalSpeechToText
import uniffi.nobodywho.RustSpeechToTextStream as InternalSpeechToTextStream

/**
 * A stream of transcript tokens from a Whisper SpeechToText run.
 *
 * ```kotlin
 * stt.transcribeFile("recording.mp3").asFlow().collect { token -> print(token) }
 * // Or get the complete transcript:
 * val text = stt.transcribeFile("recording.mp3").completed()
 * ```
 */
class SpeechToTextStream internal constructor(
    private val inner: InternalSpeechToTextStream
) : Closeable {
    /** Get the next transcript token. Returns `null` when transcription is complete. */
    suspend fun nextToken(): String? = inner.nextToken()

    /** Wait for transcription to finish and return the full transcript. */
    suspend fun completed(): String = inner.completed()

    fun asFlow(): Flow<String> = flow {
        while (true) {
            currentCoroutineContext().ensureActive()
            val token = inner.nextToken() ?: break
            emit(token)
        }
        inner.completed()
    }

    /** Free the underlying Rust resources. */
    fun destroy() = inner.destroy()
    override fun close() { destroy() }
}

/**
 * Speech-to-text handle that transcribes audio using Whisper models in ONNX format.
 *
 * ```kotlin
 * val stt = SpeechToText(source = "hf://onnx-community/whisper-base")
 * val text = stt.transcribeFile("recording.mp3").completed()
 * ```
 */
class SpeechToText(
    source: String,
    language: String? = null,
    quantization: String? = null
) : Closeable {
    private val inner: InternalSpeechToText = InternalSpeechToText(
        source = source,
        language = language,
        quantization = quantization
    )

    /**
     * Start transcribing an audio file (WAV / MP3 / FLAC).
     * Returns an [SpeechToTextStream] to consume tokens as they are generated.
     */
    fun transcribeFile(path: String): SpeechToTextStream = SpeechToTextStream(inner.transcribeFile(path))

    /**
     * Start transcribing raw i16 PCM samples (e.g. from a microphone stream).
     * [sampleRate] is the capture rate in Hz; the backend resamples to 16 kHz internally.
     */
    fun transcribePcm(samples: List<Short>, sampleRate: UInt): SpeechToTextStream =
        SpeechToTextStream(inner.transcribePcm(samples, sampleRate))

    /** Free the underlying Rust resources. */
    fun destroy() = inner.destroy()
    override fun close() { destroy() }
}
