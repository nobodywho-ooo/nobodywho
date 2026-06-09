package ai.nobodywho

import uniffi.nobodywho.SamplerBuilder
import uniffi.nobodywho.SamplerConfig

@DslMarker
annotation class SamplerDsl

/**
 * DSL receiver for building a [SamplerConfig] via [buildSampler].
 *
 * Chain shift steps then finish with a terminal step
 * (`dist`, `greedy`, `mirostatV1`, or `mirostatV2`).
 * If no terminal is called, `dist()` is used by default.
 */
@SamplerDsl
class SamplerScope {
    private var builder = SamplerBuilder()
    private var result: SamplerConfig? = null

    fun topK(topK: Int) { builder = builder.topK(topK) }
    fun topP(topP: Float, minKeep: UInt = 1u) { builder = builder.topP(topP, minKeep) }
    fun minP(minP: Float, minKeep: UInt = 1u) { builder = builder.minP(minP, minKeep) }
    fun temperature(temperature: Float) { builder = builder.temperature(temperature) }
    fun typicalP(typP: Float, minKeep: UInt = 1u) { builder = builder.typicalP(typP, minKeep) }
    fun xtc(xtcProbability: Float, xtcThreshold: Float, minKeep: UInt = 1u) { builder = builder.xtc(xtcProbability, xtcThreshold, minKeep) }
    fun grammar(grammar: String, triggerOn: String? = null, root: String = "root") { builder = builder.grammar(grammar, triggerOn, root) }

    /// Set the RNG seed used by random samplers (`dist`, `mirostatV1`, `mirostatV2`, `xtc`).
    /// `greedy` ignores it. If unset, a default seed is used.
    fun seed(value: UInt) { builder = builder.seed(value) }

    fun dry(
        multiplier: Float = 0.8f,
        base: Float = 1.75f,
        allowedLength: Int = 2,
        penaltyLastN: Int = -1,
        seqBreakers: List<String> = listOf("\n", ":", "\"", "*")
    ) { builder = builder.dry(multiplier, base, allowedLength, penaltyLastN, seqBreakers) }

    fun penalties(
        penaltyLastN: Int = 64,
        penaltyRepeat: Float = 1.0f,
        penaltyFreq: Float = 0.0f,
        penaltyPresent: Float = 0.0f
    ) { builder = builder.penalties(penaltyLastN, penaltyRepeat, penaltyFreq, penaltyPresent) }

    private fun setResult(config: SamplerConfig) {
        check(result == null) { "A terminal step was already called" }
        result = config
    }

    fun dist() { setResult(builder.dist()) }
    fun greedy() { setResult(builder.greedy()) }
    fun mirostatV1(tau: Float = 5.0f, eta: Float = 0.1f, m: Int = 100) { setResult(builder.mirostatV1(tau, eta, m)) }
    fun mirostatV2(tau: Float = 5.0f, eta: Float = 0.1f) { setResult(builder.mirostatV2(tau, eta)) }

    internal fun build(): SamplerConfig = result ?: builder.dist()
}

/**
 * Build a [SamplerConfig] using a DSL.
 *
 * ```kotlin
 * val sampler = buildSampler {
 *     topK(40)
 *     temperature(0.8f)
 *     minP(0.05f)
 *     dist()
 * }
 * ```
 */
fun buildSampler(block: SamplerScope.() -> Unit): SamplerConfig {
    return SamplerScope().apply(block).build()
}
