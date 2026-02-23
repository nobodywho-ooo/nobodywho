using System;
using Godot;
using NobodyWho.Enums;

namespace NobodyWho.Models.SamplerConfigs;

/// <summary>
/// The method config values for the <see cref="SamplerMethod.TopP"/> sampler method.
/// </summary>
/// <remarks>
/// More info can be found here:
/// <br/>• <see href="https://github.com/ggml-org/llama.cpp/blob/5c0eb5ef544aeefd81c303e03208f768e158d93c/tools/main/README.md#top-p-sampling"/>
/// <br/>• <see href="https://github.com/ggml-org/llama.cpp/blob/5c0eb5ef544aeefd81c303e03208f768e158d93c/tools/server/README.md"/>
/// <br/>• <see href="https://kojix2.github.io/llama.cr/Llama/Sampler/TopP.html"/>
/// </remarks>
public sealed class TopPConfig : IMethodConfig
{
    private readonly Resource _samplerResource;

    /// <summary>
    /// Constructs a new instance of the <see cref="TopPConfig"/>.
    /// </summary>
    /// <param name="resource">The actual sampler resource from the GDExtension.</param>
    public TopPConfig(Resource resource)
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
    public SamplerMethod Method => SamplerMethod.TopP;

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
    /// The option that limits the next token selection to a subset of tokens with a cumulative probability above a threshold <strong>P</strong>.
    /// A higher value for top-p <strong>(e.g., 0.95)</strong> will lead to more diverse text, while a lower value <strong>(e.g., 0.5)</strong> will generate more focused and conservative text.
    /// Defaults to <strong>0.95</strong>.
    /// </summary>
    public float TopP
    {
        get => _samplerResource.Get(PropertyName.TopP).As<float>();
        set => _samplerResource.Set(PropertyName.TopP, value);
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
        /// <strong>top_p</strong>
        /// </summary>
        public static readonly StringName TopP = "top_p";
    }

    #endregion Names
}