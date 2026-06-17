import Foundation
import NobodyWhoGenerated

/// A multimodal prompt composed of text, image, and audio parts.
///
/// Image and audio parts can be supplied as a file-system path or as
/// in-memory content (encoded image bytes / 16-bit PCM audio samples).
///
/// ```swift
/// let prompt = Prompt([
///     Prompt.text("Tell me what you see."),
///     Prompt.image("./photo.png"),                       // from disk
///     Prompt.image(data: pngData),                       // already in memory
///     Prompt.audio(samples: pcm, sampleRate: 16000),     // mic capture
/// ])
/// let stream = chat.ask(prompt)
/// ```
public class Prompt {
    let parts: [NobodyWhoGenerated.PromptPart]

    public init(_ parts: [NobodyWhoGenerated.PromptPart]) {
        self.parts = parts
    }

    /// Create a text part.
    public static func text(_ content: String) -> NobodyWhoGenerated.PromptPart {
        return .text(content: content)
    }

    /// Create an image part from a file path.
    public static func image(_ path: String) -> NobodyWhoGenerated.PromptPart {
        return .image(path: path)
    }

    /// Create an image part from in-memory encoded bytes (PNG, JPEG, etc.).
    public static func image(data: Data) -> NobodyWhoGenerated.PromptPart {
        return .imageBytes(data: data)
    }

    /// Create an audio part from a file path (WAV/MP3/FLAC).
    public static func audio(_ path: String) -> NobodyWhoGenerated.PromptPart {
        return .audio(path: path)
    }

    /// Create an audio part from in-memory 16-bit PCM samples + sample rate.
    /// Every current audio-capable multimodal LLM expects **16 kHz** — resample
    /// before passing in. Throws at `ask()` time if the rate doesn't match.
    public static func audio(samples: [Int16], sampleRate: UInt32 = 16000) -> NobodyWhoGenerated.PromptPart {
        return .audioPcm(samples: samples, sampleRate: sampleRate)
    }
}
