import Foundation
import NobodyWhoGenerated

public enum TtsArchitecture: String {
    case kokoro
    case pocketTts = "pocket-tts"
    case supertonic
}

public enum TtsDevice: String {
    case auto
    case cpu
    case cuda
}

/// Text-to-speech synthesizer that returns WAV bytes.
public class Tts {
    private let inner: NobodyWhoGenerated.RustTts

    /// Create a TTS synthesizer synchronously.
    public init(
        source: String,
        architecture: TtsArchitecture? = nil,
        voice: String? = nil,
        language: String? = nil,
        speed: Float? = nil,
        steps: UInt32? = nil,
        silenceDuration: Float? = nil,
        precision: String? = nil,
        temperature: Float? = nil,
        huggingfaceToken: String? = nil,
        device: TtsDevice = .auto
    ) throws {
        self.inner = try NobodyWhoGenerated.RustTts(
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
    }

    private init(inner: NobodyWhoGenerated.RustTts) {
        self.inner = inner
    }

    /// Create a TTS synthesizer asynchronously.
    public static func load(
        source: String,
        architecture: TtsArchitecture? = nil,
        voice: String? = nil,
        language: String? = nil,
        speed: Float? = nil,
        steps: UInt32? = nil,
        silenceDuration: Float? = nil,
        precision: String? = nil,
        temperature: Float? = nil,
        huggingfaceToken: String? = nil,
        device: TtsDevice = .auto
    ) async throws -> Tts {
        let inner = try await NobodyWhoGenerated.loadTts(
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
        return Tts(inner: inner)
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
