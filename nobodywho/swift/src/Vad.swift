import Foundation
import NobodyWhoGenerated

/// Voice activity detection from live, streaming audio, backed by Silero VAD.
///
/// Feed each newest chunk to `push` as it arrives — `VoiceActivityDetection` buffers the current
/// turn internally, seeded with a small pre-roll so the confirmed speech
/// isn't clipped at the start. Once `push` returns `.speechEnded`, call
/// `finish` to get that turn's audio and reset for the next one.
///
/// ```swift
/// let vad = try VoiceActivityDetection(sampleRate: 16000)
/// let event = vad.push(chunk: chunk)
/// if event == .speechEnded {
///     let audio = vad.finish()
/// }
/// ```
public class VoiceActivityDetection {
    private let inner: RustVoiceActivityDetection

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
        minSpeechDurationMs: UInt32? = nil,
        prerollDurationMs: UInt32? = nil
    ) throws {
        self.inner = try RustVoiceActivityDetection(
            source: source,
            sampleRate: sampleRate,
            threshold: threshold,
            minSilenceDurationMs: minSilenceDurationMs,
            minSpeechDurationMs: minSpeechDurationMs,
            prerollDurationMs: prerollDurationMs,
            device: nil
        )
    }

    /// Feed the newest chunk of audio (not the whole accumulated buffer —
    /// `VoiceActivityDetection` tracks the current turn internally). Returns a `VoiceActivityDetectionEvent` if
    /// this call crossed a confirmed speech/silence boundary. Throws on
    /// ONNX inference or resampling failure.
    public func push(chunk: [Int16]) throws -> VoiceActivityDetectionEvent? {
        return try inner.push(chunk: chunk)
    }

    /// Return the current turn's captured audio (from the confirmed
    /// `.speechStarted`, including a small pre-roll, through to
    /// `.speechEnded`) and reset internal state for the next turn. Empty if
    /// speech was never confirmed.
    public func finish() -> [Int16] {
        return inner.finish()
    }

    /// Detect every speech segment in a complete audio buffer, returning
    /// each segment's audio (with a short pre-roll) in order. Unlike `push`,
    /// correctly finds every segment regardless of buffer size — use this
    /// for offline/batch processing instead of live streaming.
    ///
    /// ```swift
    /// for audio in try vad.segment(samples: fullRecording) {
    ///     transcribe(audio)
    /// }
    /// ```
    public func segment(samples: [Int16]) throws -> [[Int16]] {
        return try inner.segment(samples: samples)
    }
}
