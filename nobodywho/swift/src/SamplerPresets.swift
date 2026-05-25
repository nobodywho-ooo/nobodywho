import Foundation
import NobodyWhoGenerated

/// Static factory methods for common sampler configurations.
///
/// ```swift
/// let chat = try await Chat.fromPath(
///     modelPath: "model.gguf",
///     sampler: SamplerPresets.temperature(0.7)
/// )
/// ```
public enum SamplerPresets {
    public static func `default`() -> SamplerConfig { samplerPresetDefault() }
    public static func topK(_ topK: Int32) -> SamplerConfig { samplerPresetTopK(topK: topK) }
    public static func topP(_ topP: Float) -> SamplerConfig { samplerPresetTopP(topP: topP) }
    public static func greedy() -> SamplerConfig { samplerPresetGreedy() }
    public static func temperature(_ temperature: Float) -> SamplerConfig { samplerPresetTemperature(temperature: temperature) }
    public static func dry() -> SamplerConfig { samplerPresetDry() }

    /// Constrain output to match a JSON Schema via llguidance.
    public static func constrainWithJsonSchema(_ schema: String) -> SamplerConfig { samplerPresetConstrainWithJsonSchema(schema: schema) }

    /// Constrain output to match a regular expression via llguidance.
    public static func constrainWithRegex(_ pattern: String) -> SamplerConfig { samplerPresetConstrainWithRegex(pattern: pattern) }

    /// Constrain output using a grammar (Lark or GBNF) via llguidance.
    public static func constrainWithGrammar(_ grammar: String) -> SamplerConfig { samplerPresetConstrainWithGrammar(grammar: grammar) }

    @available(*, deprecated, message: "Use constrainWithJsonSchema() for JSON output or constrainWithGrammar() for custom grammars")
    public static func json() -> SamplerConfig { samplerPresetJson() }

    @available(*, deprecated, message: "Use constrainWithGrammar() instead — it accepts both Lark and GBNF")
    public static func grammar(_ grammar: String) -> SamplerConfig { samplerPresetGrammar(grammar: grammar) }
}
