/** Minimal stubs for uniffi types needed by SamplerDsl.kt tests. */
package uniffi.nobodywho

open class SamplerConfig

open class SamplerBuilder {
    fun topK(topK: Int): SamplerBuilder = this
    fun topP(topP: Float, minKeep: UInt): SamplerBuilder = this
    fun minP(minP: Float, minKeep: UInt): SamplerBuilder = this
    fun temperature(temperature: Float): SamplerBuilder = this
    fun typicalP(typP: Float, minKeep: UInt): SamplerBuilder = this
    fun xtc(xtcProbability: Float, xtcThreshold: Float, minKeep: UInt): SamplerBuilder = this
    fun grammar(grammar: String, triggerOn: String?, root: String): SamplerBuilder = this
    fun dry(multiplier: Float, base: Float, allowedLength: Int, penaltyLastN: Int, seqBreakers: List<String>): SamplerBuilder = this
    fun penalties(penaltyLastN: Int, penaltyRepeat: Float, penaltyFreq: Float, penaltyPresent: Float): SamplerBuilder = this
    fun dist(): SamplerConfig = SamplerConfig()
    fun greedy(): SamplerConfig = SamplerConfig()
    fun mirostatV1(tau: Float, eta: Float, m: Int): SamplerConfig = SamplerConfig()
    fun mirostatV2(tau: Float, eta: Float): SamplerConfig = SamplerConfig()
}
