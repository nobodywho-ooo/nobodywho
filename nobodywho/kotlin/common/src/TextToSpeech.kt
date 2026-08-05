package ai.nobodywho

import java.io.Closeable
import uniffi.nobodywho.RustTextToSpeech as InternalTts
import uniffi.nobodywho.loadTextToSpeech as loadInternalTextToSpeech

enum class TextToSpeechArchitecture(internal val value: String) {
    KOKORO("kokoro"),
    POCKET_TTS("pocket-tts"),
    SUPERTONIC("supertonic")
}

enum class TextToSpeechDevice(internal val value: String) {
    AUTO("auto"),
    CPU("cpu"),
    CUDA("cuda")
}

/** Text-to-speech synthesizer that returns WAV bytes. */
class TextToSpeech private constructor(
    private val inner: InternalTts
) : Closeable {
    /** Create a TextToSpeech synthesizer synchronously. */
    constructor(
        source: String,
        architecture: TextToSpeechArchitecture? = null,
        voice: String? = null,
        language: String? = null,
        speed: Float? = null,
        steps: UInt? = null,
        silenceDuration: Float? = null,
        precision: String? = null,
        temperature: Float? = null,
        huggingfaceToken: String? = null,
        device: TextToSpeechDevice = TextToSpeechDevice.AUTO
    ) : this(
        InternalTts(
            source = source,
            architecture = architecture?.value,
            voice = voice,
            language = language,
            speed = speed,
            steps = steps,
            silenceDuration = silenceDuration,
            precision = precision,
            temperature = temperature,
            huggingfaceToken = huggingfaceToken,
            device = device.value
        )
    )

    companion object {
        /** Create a TextToSpeech synthesizer asynchronously. */
        suspend fun load(
            source: String,
            architecture: TextToSpeechArchitecture? = null,
            voice: String? = null,
            language: String? = null,
            speed: Float? = null,
            steps: UInt? = null,
            silenceDuration: Float? = null,
            precision: String? = null,
            temperature: Float? = null,
            huggingfaceToken: String? = null,
            device: TextToSpeechDevice = TextToSpeechDevice.AUTO
        ): TextToSpeech = TextToSpeech(
            loadInternalTextToSpeech(
                source = source,
                architecture = architecture?.value,
                voice = voice,
                language = language,
                speed = speed,
                steps = steps,
                silenceDuration = silenceDuration,
                precision = precision,
                temperature = temperature,
                huggingfaceToken = huggingfaceToken,
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
