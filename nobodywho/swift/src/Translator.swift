import Foundation
import NobodyWhoGenerated

/// A stateless one-shot translator using a TranslateGemma-compatible model.
///
/// Each `Translator` is bound to a single language pair (source → target).
/// Calls to `translate` are stateless — context is reset before each translation.
///
/// ```swift
/// let translator = try await Translator.fromPath(
///     modelPath: "translate-gemma.gguf", source: "en", target: "da")
/// let result = try await translator.translate("Hello, how are you?").completed()
/// ```
public class Translator {
    private let inner: NobodyWhoGenerated.RustTranslator

    /// - Parameters:
    ///   - model: A loaded translation model.
    ///   - source: BCP-47 source language code (e.g. "en", "de", "fr").
    ///   - target: BCP-47 target language code.
    ///   - contextSize: Maximum context size in tokens. Defaults to 4096 if nil.
    public init(model: Model, source: String, target: String, contextSize: UInt32? = nil) throws {
        self.inner = try NobodyWhoGenerated.RustTranslator(
            model: model.inner, source: source, target: target, contextSize: contextSize)
    }

    /// Create a translator directly from a model path.
    public static func fromPath(
        modelPath: String,
        source: String,
        target: String,
        useGpu: Bool = true,
        contextSize: UInt32? = nil
    ) async throws -> Translator {
        let model = try await Model.load(modelPath: modelPath, useGpu: useGpu)
        return try Translator(model: model, source: source, target: target, contextSize: contextSize)
    }

    /// Translate the given text, returning an async token stream.
    public func translate(_ text: String) -> TokenStream {
        return TokenStream(inner.translate(text: text))
    }
}
