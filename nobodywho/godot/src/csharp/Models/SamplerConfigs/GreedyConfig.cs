using NobodyWho.Enums;

namespace NobodyWho.Models.SamplerConfigs;

/// <summary>
/// The method config values for the <see cref="SamplerMethod.Greedy"/> sampler method.
/// </summary>
/// <remarks>
/// More info can be found here: <see href="https://kojix2.github.io/llama.cr/Llama/Sampler/Greedy.html"/>
/// </remarks>
public sealed class GreedyConfig : IMethodConfig
{
    /// <inheritdoc/>
    public SamplerMethod Method => SamplerMethod.Greedy;
}