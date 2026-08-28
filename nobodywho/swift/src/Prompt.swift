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
///
/// // JSON-structured prompt
/// let jsonPrompt = try Prompt.fromJson(["role": "user", "content": "Hello"])
/// let stream = chat.ask(jsonPrompt)
/// ```
public class Prompt {
    let parts: [NobodyWhoGenerated.ContentPart]?
    let jsonString: String?

    public init(_ parts: [NobodyWhoGenerated.ContentPart]) {
        self.parts = parts
        self.jsonString = nil
    }

    private init(jsonString: String) {
        self.parts = nil
        self.jsonString = jsonString
    }

    /// Create a text part.
    public static func text(_ content: String) -> NobodyWhoGenerated.ContentPart {
        return .text(text: content)
    }

    /// Create an image part from a file path.
    public static func image(_ path: String) -> NobodyWhoGenerated.ContentPart {
        return .image(path: path)
    }

    /// Create an audio part from a file path.
    public static func audio(_ path: String) -> NobodyWhoGenerated.ContentPart {
        return .audio(path: path)
    }

    /// Create a prompt from any JSON-serializable value (Dictionary, Array, String, etc.).
    public static func fromJson(_ data: Any) throws -> Prompt {
        let jsonData = try JSONSerialization.data(withJSONObject: data)
        let jsonString = String(data: jsonData, encoding: .utf8)!
        return Prompt(jsonString: jsonString)
    }
}
