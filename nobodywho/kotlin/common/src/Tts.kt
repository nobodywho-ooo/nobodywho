package ai.nobodywho

import java.io.Closeable
import uniffi.nobodywho.RustTts as InternalTts
import uniffi.nobodywho.loadTts as loadInternalTts

enum class TtsArchitecture(internal val value: String) {
    KOKORO("kokoro"),
    SUPERTONIC("supertonic")
}

enum class TtsDevice(internal val value: String) {
    AUTO("auto"),
    CPU("cpu"),
    CUDA("cuda")
}

/** Text-to-speech synthesizer that returns WAV bytes. */
class Tts private constructor(
    private val inner: InternalTts
) : Closeable {
    /** Create a TTS synthesizer synchronously. */
    constructor(
        source: String,
        architecture: TtsArchitecture? = null,
        voice: String? = null,
        language: String? = null,
        speed: Float? = null,
        steps: UInt? = null,
        silenceDuration: Float? = null,
        device: TtsDevice = TtsDevice.AUTO
    ) : this(
        InternalTts(
            source = source,
            architecture = architecture?.value,
            voice = voice,
            language = language,
            speed = speed,
            steps = steps,
            silenceDuration = silenceDuration,
            device = device.value
        )
    )

    companion object {
        /** Create a TTS synthesizer asynchronously. */
        suspend fun load(
            source: String,
            architecture: TtsArchitecture? = null,
            voice: String? = null,
            language: String? = null,
            speed: Float? = null,
            steps: UInt? = null,
            silenceDuration: Float? = null,
            device: TtsDevice = TtsDevice.AUTO
        ): Tts = Tts(
            loadInternalTts(
                source = source,
                architecture = architecture?.value,
                voice = voice,
                language = language,
                speed = speed,
                steps = steps,
                silenceDuration = silenceDuration,
                device = device.value
            )
        )
    }

    /** Synthesize text and return WAV bytes. */
    suspend fun synthesize(text: String): ByteArray = inner.synthesizeAsync(text)

    /** Synthesize text synchronously and return WAV bytes. */
    fun synthesizeSync(text: String): ByteArray = inner.synthesize(text)

    /** Free the underlying Rust resources. */
    fun destroy() = inner.destroy()
    override fun close() { destroy() }
}
