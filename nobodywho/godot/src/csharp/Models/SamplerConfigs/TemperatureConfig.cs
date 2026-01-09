using System;
using Godot;
using NobodyWho.Enums;

namespace NobodyWho.Models.SamplerConfigs;

/// <summary>
/// The method config values for the <see cref="SamplerMethod.Temperature"/> sampler method.
/// </summary>
/// <remarks>
/// More info can be found here:
/// <br/>• <see href="https://github.com/ggml-org/llama.cpp/blob/5c0eb5ef544aeefd81c303e03208f768e158d93c/tools/main/README.md#temperature"/>
/// <br/>• <see href="https://github.com/ggml-org/llama.cpp/blob/5c0eb5ef544aeefd81c303e03208f768e158d93c/tools/server/README.md"/>
/// <br/>• <see href="https://kojix2.github.io/llama.cr/Llama/Sampler/Temp.html"/>
/// </remarks>
public sealed class TemperatureConfig : IMethodConfig
{
    private readonly Resource _samplerResource;

    /// <summary>
    /// Constructs a new instance of the <see cref="TemperatureConfig"/>.
    /// </summary>
    /// <param name="resource">The actual sampler resource from the GDExtension.</param>
    public TemperatureConfig(Resource resource)
    {
        ArgumentNullException.ThrowIfNull(resource);

        if(!resource.IsClass(nameof(NobodyWhoSampler)))
        {
            throw new ArgumentException($"Resource must be of class {nameof(NobodyWhoSampler)}", nameof(resource));
        }

        if(!GodotObject.IsInstanceValid(resource) || resource.IsQueuedForDeletion())
        {
            throw new ArgumentException($"{nameof(NobodyWhoSampler)} resource node cannot be invalid or queued for deletion.", nameof(resource));
        }

        _samplerResource = resource;
    }

    /// <inheritdoc/>
    public SamplerMethod Method => SamplerMethod.Temperature;

    /// <summary>
    /// The option that controls the initial RNG seed.
    /// By setting a specific seed value, you can obtain consistent and reproducible results across multiple runs with the same input and settings.
    /// Defaults to <strong>1234</strong>.
    /// </summary>
    public uint Seed
    {
        get => _samplerResource.Get(PropertyName.Seed).AsUInt32();
        set => _samplerResource.Set(PropertyName.Seed, value);
    }

    /// <summary>
    /// The option that controls the randomness of the generated text. It affects the probability distribution of the model's output tokens.
    /// A higher temperature <strong>(e.g., 1.5)</strong> makes the output more random and creative, while a lower temperature <strong>(e.g., 0.5)</strong>
    /// makes the output more focused, deterministic, and conservative. At the extreme, a temperature of <strong>0</strong> will always pick the most likely next token, leading to identical outputs in each run.
    /// Defaults to <strong>0.8</strong>.
    /// </summary>
    public float Temperature
    {
        get => _samplerResource.Get(PropertyName.Temperature).As<float>();
        set => _samplerResource.Set(PropertyName.Temperature, value);
    }

    #region Names

    /// <summary>
    /// Cached StringNames for the properties and fields contained in this class, for fast lookup.
    /// </summary>
    private static class PropertyName
    {
        /// <summary>
        /// <strong>seed</strong>
        /// </summary>
        public static readonly StringName Seed = "seed";

        /// <summary>
        /// <strong>temperature</strong>
        /// </summary>
        public static readonly StringName Temperature = "temperature";
    }

    #endregion Names
}