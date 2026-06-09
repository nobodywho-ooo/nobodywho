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
    fun topP(topP: Double, minKeep: Int = 1) { builder = builder.topP(topP.toFloat(), minKeep.toUInt()) }
    fun minP(minP: Double, minKeep: Int = 1) { builder = builder.minP(minP.toFloat(), minKeep.toUInt()) }
    fun temperature(temperature: Double) { builder = builder.temperature(temperature.toFloat()) }
    fun typicalP(typP: Double, minKeep: Int = 1) { builder = builder.typicalP(typP.toFloat(), minKeep.toUInt()) }
    fun xtc(xtcProbability: Double, xtcThreshold: Double, minKeep: Int = 1) { builder = builder.xtc(xtcProbability.toFloat(), xtcThreshold.toFloat(), minKeep.toUInt()) }
    fun grammar(grammar: String, triggerOn: String? = null, root: String = "root") { builder = builder.grammar(grammar, triggerOn, root) }

    /// Set the RNG seed used by random samplers (`dist`, `mirostatV1`, `mirostatV2`, `xtc`).
    /// `greedy` ignores it. If unset, a default seed is used.
    fun seed(value: Int) { builder = builder.seed(value.toUInt()) }

    fun dry(
        multiplier: Double = 0.8,
        base: Double = 1.75,
        allowedLength: Int = 2,
        penaltyLastN: Int = -1,
        seqBreakers: List<String> = listOf("\n", ":", "\"", "*")
    ) { builder = builder.dry(multiplier.toFloat(), base.toFloat(), allowedLength, penaltyLastN, seqBreakers) }

    fun penalties(
        penaltyLastN: Int = 64,
        penaltyRepeat: Double = 1.0,
        penaltyFreq: Double = 0.0,
        penaltyPresent: Double = 0.0
    ) { builder = builder.penalties(penaltyLastN, penaltyRepeat.toFloat(), penaltyFreq.toFloat(), penaltyPresent.toFloat()) }

    private fun setResult(config: SamplerConfig) {
        check(result == null) { "A terminal step was already called" }
        result = config
    }

    fun dist() { setResult(builder.dist()) }
    fun greedy() { setResult(builder.greedy()) }
    fun mirostatV1(tau: Double = 5.0, eta: Double = 0.1, m: Int = 100) { setResult(builder.mirostatV1(tau.toFloat(), eta.toFloat(), m)) }
    fun mirostatV2(tau: Double = 5.0, eta: Double = 0.1) { setResult(builder.mirostatV2(tau.toFloat(), eta.toFloat())) }

    internal fun build(): SamplerConfig = result ?: builder.dist()
}

/**
 * Build a [SamplerConfig] using a DSL.
 *
 * ```kotlin
 * val sampler = buildSampler {
 *     topK(40)
 *     temperature(0.8)
 *     minP(0.05)
 *     dist()
 * }
 * ```
 */
fun buildSampler(block: SamplerScope.() -> Unit): SamplerConfig {
    return SamplerScope().apply(block).build()
}
