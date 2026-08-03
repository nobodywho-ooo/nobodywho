import Foundation
import NobodyWhoGenerated

/// Voice activity detection from live, streaming audio, backed by Silero VAD.
///
/// Feed each newest chunk to `push` as it arrives — `Vad` buffers the current
/// turn internally, seeded with a small pre-roll so the confirmed speech
/// isn't clipped at the start. Once `push` returns `.speechEnded`, call
/// `finish` to get that turn's audio and reset for the next one.
///
/// ```swift
/// let vad = try Vad(sampleRate: 16000)
/// let event = vad.push(chunk: chunk)
/// if event == .speechEnded {
///     let audio = vad.finish()
/// }
/// ```
public class Vad {
    private let inner: RustVad

    /// - Parameters:
    ///   - sampleRate: Rate of the audio you'll pass to `push`. Anything
    ///     other than 16kHz is resampled internally.
    ///   - source: HuggingFace repo (`hf://owner/repo`) or local directory
    ///     for the Silero VAD ONNX model. Pass `nil` to use the default
    ///     (`hf://onnx-community/silero-vad`).
    public init(
        sampleRate: UInt32,
        source: String? = nil,
        threshold: Float? = nil,
        minSilenceDurationMs: UInt32? = nil,
        minSpeechDurationMs: UInt32? = nil
    ) throws {
        self.inner = try RustVad(
            source: source,
            sampleRate: sampleRate,
            threshold: threshold,
            minSilenceDurationMs: minSilenceDurationMs,
            minSpeechDurationMs: minSpeechDurationMs,
            device: nil
        )
    }

    /// Feed the newest chunk of audio (not the whole accumulated buffer —
    /// `Vad` tracks the current turn internally). Returns a `VadEvent` if
    /// this call crossed a confirmed speech/silence boundary.
    public func push(chunk: [Int16]) -> VadEvent? {
        return inner.push(chunk: chunk)
    }

    /// Return the current turn's captured audio (from the confirmed
    /// `.speechStarted`, including a small pre-roll, through to
    /// `.speechEnded`) and reset internal state for the next turn. Empty if
    /// speech was never confirmed.
    public func finish() -> [Int16] {
        return inner.finish()
    }
}
