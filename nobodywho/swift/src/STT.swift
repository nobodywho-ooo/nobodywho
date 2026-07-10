import Foundation
import NobodyWhoGenerated

/// A stream of transcript tokens from a Whisper STT run.
///
/// ```swift
/// let stream = try stt.transcribeFile(path: "recording.mp3")
/// for try await token in stream {
///     print(token, terminator: "")
/// }
/// // Or collect the full transcript:
/// let text = try await stt.transcribeFile(path: "recording.mp3").completed()
/// ```
public struct SttStream: AsyncSequence {
    public typealias Element = String

    let inner: RustSttStream

    init(_ inner: RustSttStream) {
        self.inner = inner
    }

    /// Get the next token. Returns nil when transcription is complete.
    public func nextToken() async throws -> String? {
        return try await inner.nextToken()
    }

    /// Wait for transcription to finish and return the full transcript.
    public func completed() async throws -> String {
        return try await inner.completed()
    }

    public func makeAsyncIterator() -> AsyncIterator {
        return AsyncIterator(inner: inner)
    }

    public struct AsyncIterator: AsyncIteratorProtocol {
        let inner: RustSttStream

        public mutating func next() async throws -> String? {
            return try await inner.nextToken()
        }
    }
}

/// Speech-to-text using a Whisper ONNX model.
///
/// `source` is a HuggingFace repo (`hf://owner/repo`, e.g.
/// `"hf://onnx-community/whisper-base"`) or a local directory path. The model is
/// downloaded and cached on first use.
///
/// ```swift
/// let stt = try STT(source: "hf://onnx-community/whisper-base")
/// let text = try await stt.transcribeFile(path: "recording.mp3").completed()
///
/// // Stream tokens as they arrive:
/// for try await token in stt.transcribeFile(path: "recording.mp3") {
///     print(token, terminator: "")
/// }
/// ```
public class STT {
    private let inner: RustStt

    /// - Parameters:
    ///   - source: HuggingFace repo ID or local directory path.
    ///   - language: ISO 639-1 language code (e.g. `"en"`). Pass `nil` to auto-detect.
    ///   - quantization: ONNX precision variant to download and load: one of
    ///     `"default"`, `"fp16"`, `"int8"`, `"uint8"`, `"bnb4"`, `"q4"`, `"q4f16"`, `"quantized"`.
    ///     Pass `nil` to use `"default"`.
    public init(source: String, language: String? = nil, quantization: String? = nil) throws {
        self.inner = try RustStt(source: source, language: language, quantization: quantization)
    }

    /// Transcribe an audio file (WAV / MP3). Returns an `SttStream`.
    public func transcribeFile(path: String) throws -> SttStream {
        return SttStream(try inner.transcribeFile(path: path))
    }

    /// Transcribe raw i16 PCM samples. Returns an `SttStream`.
    public func transcribePcm(samples: [Int16], sampleRate: UInt32) throws -> SttStream {
        return SttStream(try inner.transcribePcm(samples: samples, sampleRate: sampleRate))
    }
}
