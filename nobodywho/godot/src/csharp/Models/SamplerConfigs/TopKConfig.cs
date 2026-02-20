using System;
using Godot;
using NobodyWho.Enums;

namespace NobodyWho.Models.SamplerConfigs;

/// <summary>
/// The method config values for the <see cref="SamplerMethod.TopK"/> sampler method.
/// </summary>
/// <remarks>
/// More info can be found here:
/// <br/>• <see href="https://github.com/ggml-org/llama.cpp/blob/5c0eb5ef544aeefd81c303e03208f768e158d93c/tools/main/README.md#top-k-sampling"/>
/// <br/>• <see href="https://github.com/ggml-org/llama.cpp/blob/5c0eb5ef544aeefd81c303e03208f768e158d93c/tools/server/README.md"/>
/// <br/>• <see href="https://kojix2.github.io/llama.cr/Llama/Sampler/TopK.html"/>
/// </remarks>
public sealed class TopKConfig : IMethodConfig
{
    private readonly Resource _samplerResource;

    /// <summary>
    /// Constructs a new instance of the <see cref="TopKConfig"/>.
    /// </summary>
    /// <param name="resource">The actual sampler resource from the GDExtension.</param>
    public TopKConfig(Resource resource)
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
    public SamplerMethod Method => SamplerMethod.TopK;

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
    /// The option that limits the next token to the <strong>K</strong> most probably tokens.
    /// A higher value for top-k <strong>(e.g., 100)</strong> will consider more tokens and lead to more diverse text,
    /// while a lower value <strong>(e.g., 10)</strong> will focus on the most probable tokens and generate more conservative text.
    /// Defaults to <strong>40</strong>
    /// </summary>
    public int TopK
    {
        get => _samplerResource.Get(PropertyName.TopK).AsInt32();
        set => _samplerResource.Set(PropertyName.TopK, value);
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
        /// <strong>top_k</strong>
        /// </summary>
        public static readonly StringName TopK = "top_k";
    }

    #endregion Names
}