import Foundation
import NobodyWhoGenerated

public enum TextToSpeechArchitecture: String {
    case kokoro
    case pocketTts = "pocket-tts"
    case supertonic
}

public enum TextToSpeechDevice: String {
    case auto
    case cpu
    case cuda
}

/// Text-to-speech synthesizer that returns WAV bytes.
public class TextToSpeech {
    private let inner: NobodyWhoGenerated.RustTextToSpeech

    /// Create a TextToSpeech synthesizer synchronously.
    public init(
        source: String,
        architecture: TextToSpeechArchitecture? = nil,
        voice: String? = nil,
        language: String? = nil,
        speed: Float? = nil,
        steps: UInt32? = nil,
        silenceDuration: Float? = nil,
        precision: String? = nil,
        temperature: Float? = nil,
        huggingfaceToken: String? = nil,
        device: TextToSpeechDevice = .auto
    ) throws {
        let options = NobodyWhoGenerated.TextToSpeechOptions(
            source: source,
            architecture: architecture?.rawValue,
            voice: voice,
            language: language,
            speed: speed,
            steps: steps,
            silenceDuration: silenceDuration,
            precision: precision,
            temperature: temperature,
            huggingfaceToken: huggingfaceToken,
            device: device.rawValue
        )
        self.inner = try NobodyWhoGenerated.RustTextToSpeech(options: options)
    }

    private init(inner: NobodyWhoGenerated.RustTextToSpeech) {
        self.inner = inner
    }

    /// Create a TextToSpeech synthesizer asynchronously.
    public static func load(
        source: String,
        architecture: TextToSpeechArchitecture? = nil,
        voice: String? = nil,
        language: String? = nil,
        speed: Float? = nil,
        steps: UInt32? = nil,
        silenceDuration: Float? = nil,
        precision: String? = nil,
        temperature: Float? = nil,
        huggingfaceToken: String? = nil,
        device: TextToSpeechDevice = .auto
    ) async throws -> TextToSpeech {
        let options = NobodyWhoGenerated.TextToSpeechOptions(
            source: source,
            architecture: architecture?.rawValue,
            voice: voice,
            language: language,
            speed: speed,
            steps: steps,
            silenceDuration: silenceDuration,
            precision: precision,
            temperature: temperature,
            huggingfaceToken: huggingfaceToken,
            device: device.rawValue
        )
        let inner = try await NobodyWhoGenerated.RustTextToSpeech.load(options: options)
        return TextToSpeech(inner: inner)
    }

    /// Synthesize text and return WAV bytes.
    public func synthesize(_ text: String) async throws -> Data {
        try await inner.synthesizeAsync(text: text)
    }

    /// Synthesize text synchronously and return WAV bytes.
    public func synthesizeSync(_ text: String) throws -> Data {
        try inner.synthesize(text: text)
    }
}
