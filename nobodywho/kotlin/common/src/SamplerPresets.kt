package ai.nobodywho

import uniffi.nobodywho.samplerPresetConstrainWithGrammar
import uniffi.nobodywho.samplerPresetConstrainWithJsonSchema
import uniffi.nobodywho.samplerPresetConstrainWithRegex
import uniffi.nobodywho.samplerPresetDefault
import uniffi.nobodywho.samplerPresetDry
import uniffi.nobodywho.samplerPresetGrammar
import uniffi.nobodywho.samplerPresetGreedy
import uniffi.nobodywho.samplerPresetJson
import uniffi.nobodywho.samplerPresetTemperature
import uniffi.nobodywho.samplerPresetTopK
import uniffi.nobodywho.samplerPresetTopP

/**
 * Factory methods for common sampler configurations.
 *
 * ```kotlin
 * val sampler = SamplerPresets.temperature(0.7f)
 * val chat = Chat(model = model, sampler = sampler)
 * ```
 */
object SamplerPresets {
    fun default(): SamplerConfig = samplerPresetDefault()
    fun topK(topK: Int): SamplerConfig = samplerPresetTopK(topK)
    fun topP(topP: Float): SamplerConfig = samplerPresetTopP(topP)
    fun greedy(): SamplerConfig = samplerPresetGreedy()
    fun temperature(temperature: Float): SamplerConfig = samplerPresetTemperature(temperature)
    fun dry(): SamplerConfig = samplerPresetDry()
    fun json(): SamplerConfig = samplerPresetJson()
    fun constrainWithJsonSchema(schema: String): SamplerConfig = samplerPresetConstrainWithJsonSchema(schema)
    fun constrainWithRegex(pattern: String): SamplerConfig = samplerPresetConstrainWithRegex(pattern)
    fun constrainWithGrammar(grammar: String): SamplerConfig = samplerPresetConstrainWithGrammar(grammar)
    @Deprecated("Use constrainWithGrammar() instead", ReplaceWith("constrainWithGrammar(grammar)"))
    fun grammar(grammar: String): SamplerConfig = samplerPresetGrammar(grammar)
}
