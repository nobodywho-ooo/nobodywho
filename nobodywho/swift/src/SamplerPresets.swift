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
    public static func json() -> SamplerConfig { samplerPresetJson() }
    public static func grammar(_ grammar: String) -> SamplerConfig { samplerPresetGrammar(grammar: grammar) }
}
