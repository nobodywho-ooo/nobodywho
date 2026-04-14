import Foundation
import NobodyWhoGenerated

/// A multimodal prompt composed of text, image, and audio parts.
///
/// ```swift
/// let prompt = Prompt([
///     Prompt.text("Tell me what you see."),
///     Prompt.image("./photo.png"),
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

    /// Create an audio part from a file path.
    public static func audio(_ path: String) -> NobodyWhoGenerated.PromptPart {
        return .audio(path: path)
    }
}
