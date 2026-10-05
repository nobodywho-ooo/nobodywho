using System;
using System.Collections.Generic;
using System.Linq;

namespace NobodyWho;

/// <summary>
/// A complete sampler chain: how the next token is picked from the model's output.
/// Build one with <see cref="SamplerBuilder"/> or take a ready-made one from
/// <see cref="SamplerPresets"/>.
/// </summary>
public sealed class SamplerConfig : IDisposable
{
    internal Native.SamplerConfig Inner { get; }

    internal SamplerConfig(Native.SamplerConfig inner) => Inner = inner;

    /// <summary>Deserialize a sampler configuration from JSON, as written by <see cref="ToJson"/>.</summary>
    /// <exception cref="NobodyWhoException">The JSON is not a valid sampler configuration.</exception>
    public static SamplerConfig FromJson(string json) =>
        new(Checked.Call(() => Native.SamplerConfig.FromJson(json)));

    /// <summary>Serialize the sampler configuration to JSON.</summary>
    public string ToJson() => Checked.Call(() => Inner.ToJson());

    /// <inheritdoc/>
    public override string ToString() => ToJson();

    /// <summary>Free the underlying native resources.</summary>
    public void Dispose() => Inner.Dispose();
}

/// <summary>
/// Builds a custom sampler chain. Each step returns a new builder, so a builder can be reused
/// as the base for several configurations. Finish with a sampling step (<see cref="Dist"/>,
/// <see cref="Greedy"/>, <see cref="MirostatV1"/> or <see cref="MirostatV2"/>).
/// </summary>
/// <example>
/// <code>
/// var sampler = new SamplerBuilder()
///     .TopK(40)
///     .Temperature(0.8f)
///     .Dist();
/// </code>
/// </example>
/// <remarks>
/// Constraining steps (<see cref="ConstrainWithJsonSchema"/>, <see cref="ConstrainWithRegex"/>,
/// <see cref="ConstrainWithGrammar"/>, <see cref="Json"/>) always run before the other steps,
/// wherever they appear in the chain.
/// </remarks>
public sealed class SamplerBuilder
{
    private readonly Native.SamplerBuilder _inner;

    /// <summary>Start an empty sampler chain.</summary>
    public SamplerBuilder() : this(new Native.SamplerBuilder()) { }

    private SamplerBuilder(Native.SamplerBuilder inner) => _inner = inner;

    private SamplerBuilder Next(Func<Native.SamplerBuilder, Native.SamplerBuilder> step) => new(step(_inner));

    /// <summary>Keep only the <paramref name="topK"/> most probable tokens.</summary>
    public SamplerBuilder TopK(int topK) => Next(b => b.TopK(topK));

    /// <summary>Keep tokens whose cumulative probability is below <paramref name="topP"/>.</summary>
    public SamplerBuilder TopP(float topP, int minKeep = 1) =>
        Next(b => b.TopP(topP, Checked.ToUInt(minKeep, nameof(minKeep))));

    /// <summary>Keep tokens with probability above <paramref name="minP"/> times that of the most likely token.</summary>
    public SamplerBuilder MinP(float minP, int minKeep = 1) =>
        Next(b => b.MinP(minP, Checked.ToUInt(minKeep, nameof(minKeep))));

    /// <summary>Apply temperature scaling to the probability distribution.</summary>
    public SamplerBuilder Temperature(float temperature) => Next(b => b.Temperature(temperature));

    /// <summary>XTC sampler, which probabilistically excludes high-probability tokens.</summary>
    public SamplerBuilder Xtc(float xtcProbability, float xtcThreshold, int minKeep = 1) =>
        Next(b => b.Xtc(xtcProbability, xtcThreshold, Checked.ToUInt(minKeep, nameof(minKeep))));

    /// <summary>
    /// Set the RNG seed used by random samplers (<see cref="Dist"/>, Mirostat, <see cref="Xtc"/>).
    /// <see cref="Greedy"/> ignores it. If unset, a default seed is used.
    /// </summary>
    public SamplerBuilder Seed(uint seed) => Next(b => b.Seed(seed));

    /// <summary>Typical sampling: keeps tokens close to the expected information content.</summary>
    public SamplerBuilder TypicalP(float typicalP, int minKeep = 1) =>
        Next(b => b.TypicalP(typicalP, Checked.ToUInt(minKeep, nameof(minKeep))));

    /// <summary>Constrain output to a JSON schema, given as a JSON string.</summary>
    public SamplerBuilder ConstrainWithJsonSchema(string schema) => Next(b => b.ConstrainWithJsonSchema(schema));

    /// <summary>Constrain output to a regular expression.</summary>
    public SamplerBuilder ConstrainWithRegex(string pattern) => Next(b => b.ConstrainWithRegex(pattern));

    /// <summary>Constrain output to a grammar, given as either Lark or GBNF.</summary>
    public SamplerBuilder ConstrainWithGrammar(string grammar) => Next(b => b.ConstrainWithGrammar(grammar));

    /// <summary>
    /// Constrain output to a JSON object of any shape. Use <see cref="ConstrainWithJsonSchema"/>
    /// to pin down the structure too.
    /// </summary>
    public SamplerBuilder Json() => Next(b => b.Json());

    /// <summary>DRY ("don't repeat yourself") sampler, which reduces repetition.</summary>
    /// <param name="multiplier">Penalty strength.</param>
    /// <param name="base">Base of the exponential penalty growth.</param>
    /// <param name="allowedLength">Repeated sequences up to this length are not penalized.</param>
    /// <param name="penaltyLastN">How many recent tokens to scan; <c>-1</c> scans the whole context.</param>
    /// <param name="sequenceBreakers">Strings that end a repeated sequence. Defaults to newline, <c>:</c>, <c>"</c> and <c>*</c>.</param>
    public SamplerBuilder Dry(
        float multiplier = 0.8f,
        float @base = 1.75f,
        int allowedLength = 2,
        int penaltyLastN = -1,
        IEnumerable<string>? sequenceBreakers = null) =>
        Next(b => b.Dry(multiplier, @base, allowedLength, penaltyLastN,
            (sequenceBreakers ?? DefaultSequenceBreakers).ToArray()));

    private static readonly string[] DefaultSequenceBreakers = ["\n", ":", "\"", "*"];

    /// <summary>Apply repetition penalties to discourage repeated tokens.</summary>
    public SamplerBuilder Penalties(int penaltyLastN = 64, float penaltyRepeat = 1f, float penaltyFrequency = 0f, float penaltyPresent = 0f) =>
        Next(b => b.Penalties(penaltyLastN, penaltyRepeat, penaltyFrequency, penaltyPresent));

    /// <summary>
    /// Dynamic temperature scaling (entropy sampling, <see href="https://arxiv.org/abs/2309.02772"/>).
    /// The final temperature lies in <c>[temperature - delta, temperature + delta]</c> and is
    /// computed as <c>entropy ^ exponent</c>.
    /// </summary>
    public SamplerBuilder DynamicTemperature(float temperature, float delta, float exponent) =>
        Next(b => b.DynamicTemperature(temperature, delta, exponent));

    /// <summary>
    /// Top-nσ sampling (<see href="https://arxiv.org/pdf/2411.07641"/>): keep tokens within
    /// <paramref name="n"/> standard deviations of the top logit.
    /// </summary>
    public SamplerBuilder TopNSigma(float n) => Next(b => b.TopNSigma(n));

    /// <summary>
    /// Modify the likelihood of specific tokens. A bias above 0 makes a token more likely;
    /// <see cref="float.NegativeInfinity"/> bans it.
    /// </summary>
    public SamplerBuilder LogitBias(IReadOnlyDictionary<int, float> biases) =>
        Next(b => b.LogitBias(biases.ToDictionary(kv => kv.Key, kv => kv.Value)));

    /// <summary>Finish the chain by sampling from the distribution (weighted random selection).</summary>
    public SamplerConfig Dist() => new(_inner.Dist());

    /// <summary>Finish the chain by always picking the most probable token.</summary>
    public SamplerConfig Greedy() => new(_inner.Greedy());

    /// <summary>Finish the chain with Mirostat v1 (perplexity-controlled sampling).</summary>
    public SamplerConfig MirostatV1(float tau = 5f, float eta = 0.1f, int m = 100) => new(_inner.MirostatV1(tau, eta, m));

    /// <summary>Finish the chain with Mirostat v2 (perplexity-controlled sampling).</summary>
    public SamplerConfig MirostatV2(float tau = 5f, float eta = 0.1f) => new(_inner.MirostatV2(tau, eta));
}

/// <summary>
/// Ready-made sampler configurations.
/// </summary>
/// <remarks>
/// Every preset builds on <see cref="Default"/> and adds its own step on top, replacing the
/// default step of the same kind if there is one. <see cref="Greedy"/> is the exception: it
/// always picks the most probable token, so it needs no steps.
/// </remarks>
/// <example>
/// <code>
/// using var chat = new Chat(model, sampler: SamplerPresets.Temperature(0.7f));
/// </code>
/// </example>
public static class SamplerPresets
{
    /// <summary>The default sampler configuration.</summary>
    public static SamplerConfig Default() => new(Native.NativeMethods.SamplerPresetDefault());

    /// <summary>The default steps, with top-k overridden.</summary>
    public static SamplerConfig TopK(int topK) => new(Native.NativeMethods.SamplerPresetTopK(topK));

    /// <summary>The default steps, with nucleus (top-p) sampling overridden.</summary>
    public static SamplerConfig TopP(float topP) => new(Native.NativeMethods.SamplerPresetTopP(topP));

    /// <summary>Always pick the most probable token.</summary>
    public static SamplerConfig Greedy() => new(Native.NativeMethods.SamplerPresetGreedy());

    /// <summary>The default steps, with the temperature overridden.</summary>
    public static SamplerConfig Temperature(float temperature) =>
        new(Native.NativeMethods.SamplerPresetTemperature(temperature));

    /// <summary>The default steps plus DRY, which reduces repetition.</summary>
    public static SamplerConfig Dry() => new(Native.NativeMethods.SamplerPresetDry());

    /// <summary>Constrain output to a JSON object of any shape.</summary>
    public static SamplerConfig Json() => new(Native.NativeMethods.SamplerPresetJson());

    /// <summary>Constrain output to a JSON schema, given as a JSON string.</summary>
    public static SamplerConfig ConstrainWithJsonSchema(string schema) =>
        new(Native.NativeMethods.SamplerPresetConstrainWithJsonSchema(schema));

    /// <summary>Constrain output to a regular expression.</summary>
    public static SamplerConfig ConstrainWithRegex(string pattern) =>
        new(Native.NativeMethods.SamplerPresetConstrainWithRegex(pattern));

    /// <summary>Constrain output to a grammar, given as either Lark or GBNF.</summary>
    public static SamplerConfig ConstrainWithGrammar(string grammar) =>
        new(Native.NativeMethods.SamplerPresetConstrainWithGrammar(grammar));
}
