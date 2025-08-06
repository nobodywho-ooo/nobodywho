using System;
using Godot;
using NobodyWho.Enums;

namespace NobodyWho.Models.SamplerConfigs;

/// <summary>
/// The method config values for the <see cref="SamplerMethod.DRY"/> (<strong>Don't Repeat Yourself</strong>) sampler method.
/// </summary>
/// <remarks>
/// More info can be found here:
/// <br/>• <see href="https://github.com/ggml-org/llama.cpp/blob/5c0eb5ef544aeefd81c303e03208f768e158d93c/tools/main/README.md#dry-repetition-penalty"/>
/// <br/>• <see href="https://github.com/ggml-org/llama.cpp/blob/5c0eb5ef544aeefd81c303e03208f768e158d93c/tools/server/README.md"/>
/// </remarks>
public sealed class DryConfig : IMethodConfig
{
    private readonly Resource _samplerResource;

    /// <summary>
    /// Constructs a new instance of the <see cref="DryConfig"/>.
    /// </summary>
    /// <param name="resource">The actual sampler resource from the GDExtension.</param>
    public DryConfig(Resource resource)
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
    public SamplerMethod Method => SamplerMethod.DRY;

    /// <summary>
    /// The option that controls the maximum length of repeated sequences that will not be penalized.
    /// Repetitions shorter than or equal to this length are not penalized, allowing for natural repetitions of short phrases or common words.
    /// Defaults to <strong>2</strong>.
    /// </summary>
    public int DryAllowedLength
    {
        get => _samplerResource.Get(PropertyName.DryAllowedLength).AsInt32();
        set => _samplerResource.Set(PropertyName.DryAllowedLength, value);
    }

    /// <summary>
    /// The option that controls the base value for exponential penalty calculation in DRY sampling.
    /// Higher values lead to more aggressive penalization of repetitions. Defaults to <strong>1.75</strong>.
    /// </summary>
    public float DryBase
    {
        get => _samplerResource.Get(PropertyName.DryBase).As<float>();
        set => _samplerResource.Set(PropertyName.DryBase, value);
    }

    /// <summary>
    /// The option that controls the strength of the DRY sampling effect. A value of <strong>0.0</strong> disables DRY sampling,
    /// while higher values increases its influence. Defaults to <strong>0.0</strong>.
    /// </summary>
    public float DryMultiplier
    {
        get => _samplerResource.Get(PropertyName.DryMultiplier).As<float>();
        set => _samplerResource.Set(PropertyName.DryMultiplier, value);
    }

    /// <summary>
    /// The option that controls how many recent tokens to consider when applying the DRY penalty.
    /// A value of -1 considers the entire context. Use a positive value to limit the consideration to a specific number of recent tokens.
    /// Defaults to <strong>-1</strong>.
    /// </summary>
    public int DryPenaltyLastN
    {
        get => _samplerResource.Get(PropertyName.DryPenaltyLastN).AsInt32();
        set => _samplerResource.Set(PropertyName.DryPenaltyLastN, value);
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

    #region Names

    /// <summary>
    /// Cached StringNames for the properties and fields contained in this class, for fast lookup.
    /// </summary>
    private static class PropertyName
    {
        /// <summary>
        /// <strong>dry_allowed_length</strong>
        /// </summary>
        public static readonly StringName DryAllowedLength = "dry_allowed_length";

        /// <summary>
        /// <strong>dry_base</strong>
        /// </summary>
        public static readonly StringName DryBase = "dry_base";

        /// <summary>
        /// <strong>dry_multiplier</strong>
        /// </summary>
        public static readonly StringName DryMultiplier = "dry_multiplier";

        /// <summary>
        /// <strong>dry_penalty_last_n</strong>
        /// </summary>
        public static readonly StringName DryPenaltyLastN = "dry_penalty_last_n";

        /// <summary>
        /// <strong>seed</strong>
        /// </summary>
        public static readonly StringName Seed = "seed";
    }

    #endregion Names
}