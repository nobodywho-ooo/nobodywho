package ai.nobodywho

import java.io.Closeable
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import uniffi.nobodywho.RustStt as InternalStt
import uniffi.nobodywho.RustSttStream as InternalSttStream

/**
 * A stream of transcript tokens from a Whisper STT run.
 *
 * ```kotlin
 * stt.transcribeFile("recording.mp3").asFlow().collect { token -> print(token) }
 * // Or get the complete transcript:
 * val text = stt.transcribeFile("recording.mp3").completed()
 * ```
 */
class SttStream internal constructor(
    private val inner: InternalSttStream
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
 * val stt = Stt(source = "hf://onnx-community/whisper-base")
 * val text = stt.transcribeFile("recording.mp3").completed()
 * ```
 */
class Stt(
    source: String,
    language: String? = null,
    quantization: String? = null
) : Closeable {
    private val inner: InternalStt = InternalStt(
        source = source,
        language = language,
        quantization = quantization
    )

    /**
     * Start transcribing an audio file (WAV / MP3 / FLAC).
     * Returns an [SttStream] to consume tokens as they are generated.
     */
    fun transcribeFile(path: String): SttStream = SttStream(inner.transcribeFile(path))

    /**
     * Start transcribing raw i16 PCM samples (e.g. from a microphone stream).
     * [sampleRate] is the capture rate in Hz; the backend resamples to 16 kHz internally.
     */
    fun transcribePcm(samples: List<Short>, sampleRate: UInt): SttStream =
        SttStream(inner.transcribePcm(samples, sampleRate))

    /** Free the underlying Rust resources. */
    fun destroy() = inner.destroy()
    override fun close() { destroy() }
}
