package ai.nobodywho

import java.io.Closeable
import uniffi.nobodywho.RustTranslator as InternalTranslator

/**
 * A stateless one-shot translator using a TranslateGemma-compatible model.
 *
 * Each `Translator` is bound to a single language pair (source → target).
 * Calls to `translate()` are stateless — context is reset before each translation.
 *
 * ```kotlin
 * val translator = Translator.fromPath("translate-gemma.gguf", source = "en", target = "da")
 * val stream = translator.translate("Hello, how are you?")
 * println(stream.completed())
 * ```
 */
class Translator(
    model: Model,
    source: String,
    target: String,
    contextSize: UInt? = null,
) : Closeable {
    private val inner: InternalTranslator =
        InternalTranslator(model.inner, source, target, contextSize)

    companion object {
        /** Create a translator directly from a model path. */
        suspend fun fromPath(
            modelPath: String,
            source: String,
            target: String,
            useGpu: Boolean = true,
            contextSize: UInt? = null,
        ): Translator = Translator(Model.load(modelPath, useGpu), source, target, contextSize)
    }

    /**
     * Translate the given text.
     *
     * Returns a [TokenStream] that yields translation tokens as they are generated.
     */
    fun translate(text: String): TokenStream = TokenStream(inner.translate(text))

    /** Free the underlying Rust resources. */
    fun destroy() = inner.destroy()
    override fun close() { destroy() }
}
