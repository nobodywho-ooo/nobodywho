using System;
using Godot;
using NobodyWho.Enums;

namespace NobodyWho.Models.SamplerConfigs;

/// <summary>
/// The method config values for the <see cref="SamplerMethod.XTC"/> (E<strong>X</strong>clude <strong>T</strong>op <strong>C</strong>hoices) sampler method.
/// </summary>
/// <remarks>
/// More info can be found here:
/// <br/>• <see href="https://github.com/ggml-org/llama.cpp/blob/5c0eb5ef544aeefd81c303e03208f768e158d93c/tools/main/README.md#xtc-sampling"/>
/// <br/>• <see href="https://github.com/ggml-org/llama.cpp/blob/5c0eb5ef544aeefd81c303e03208f768e158d93c/tools/server/README.md"/>
/// <br/>• <see href="https://kojix2.github.io/llama.cr/Llama/Sampler/Xtc.html"/>
/// </remarks>
public sealed class XTCConfig : IMethodConfig
{
    private readonly Resource _samplerResource;

    /// <summary>
    /// Constructs a new instance of the <see cref="XTCConfig"/>.
    /// </summary>
    /// <param name="resource">The actual sampler resource from the GDExtension.</param>
    public XTCConfig(Resource resource)
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
    public SamplerMethod Method => SamplerMethod.XTC;

    /// <summary>
    /// The option, if greater than 0, that forces the sample to return <strong>N</strong> possible tokens at minimum.
    /// Defaults to <strong>0</strong>.
    /// </summary>
    public uint MinKeep
    {
        get => _samplerResource.Get(PropertyName.MinKeep).AsUInt32();
        set => _samplerResource.Set(PropertyName.MinKeep, value);
    }

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
    /// The option that sets the chance for token removal (checked once on sampler start).
    /// Defaults to <strong>0.00</strong>.
    /// </summary>
    public float XTCProbability
    {
        get => _samplerResource.Get(PropertyName.XTCProbability).As<float>();
        set => _samplerResource.Set(PropertyName.XTCProbability, value);
    }

    /// <summary>
    /// The option that sets a minimum probability threshold for tokens to be removed.
    /// Defaults to <strong>0.10</strong>.
    /// </summary>
    public float XTCThreshold
    {
        get => _samplerResource.Get(PropertyName.XTCThreshold).As<float>();
        set => _samplerResource.Set(PropertyName.XTCThreshold, value);
    }

    #region Names

    /// <summary>
    /// Cached StringNames for the properties and fields contained in this class, for fast lookup.
    /// </summary>
    private static class PropertyName
    {
        /// <summary>
        /// <strong>min_keep</strong>
        /// </summary>
        public static readonly StringName MinKeep = "min_keep";

        /// <summary>
        /// <strong>seed</strong>
        /// </summary>
        public static readonly StringName Seed = "seed";

        /// <summary>
        /// <strong>xtc_probability</strong>
        /// </summary>
        public static readonly StringName XTCProbability = "xtc_probability";

        /// <summary>
        /// <strong>xtc_threshold</strong>
        /// </summary>
        public static readonly StringName XTCThreshold = "xtc_threshold";
    }

    #endregion Names
}