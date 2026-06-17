import Foundation
import NobodyWhoGenerated

/// A multimodal prompt composed of text, image, and audio parts.
///
/// `image` / `audio` reference files on disk. `imageBytes` accepts encoded
/// image bytes already in memory (PNG/JPEG/etc.). `audioPcm` accepts 16-bit
/// PCM samples + sample rate (typically 16 kHz, the default).
///
/// ```swift
/// let prompt = Prompt([
///     Prompt.text("Tell me what you see."),
///     Prompt.image("./photo.png"),
///     Prompt.imageBytes(pngData),
///     Prompt.audioPcm(samples: pcm, sampleRate: 16000),
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
    public static func imageBytes(_ data: Data) -> NobodyWhoGenerated.PromptPart {
        return .imageBytes(data: data)
    }

    /// Create an audio part from a file path (WAV/MP3/FLAC).
    public static func audio(_ path: String) -> NobodyWhoGenerated.PromptPart {
        return .audio(path: path)
    }

    /// Create an audio part from in-memory 16-bit PCM samples + sample rate.
    /// Every current audio-capable multimodal LLM expects **16 kHz** — resample
    /// before passing in. Throws at `ask()` time if the rate doesn't match.
    public static func audioPcm(samples: [Int16], sampleRate: UInt32 = 16000) -> NobodyWhoGenerated.PromptPart {
        return .audioPcm(samples: samples, sampleRate: sampleRate)
    }
}
