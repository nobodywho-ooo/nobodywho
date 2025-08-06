using System;
using Godot;
using NobodyWho.Enums;

namespace NobodyWho.Models.SamplerConfigs;

/// <summary>
/// The method config values for the <see cref="SamplerMethod.MirostatV2"/> sampler method.
/// </summary>
/// <remarks>
/// More info can be found here:
/// <br/>• <see href="https://github.com/ggml-org/llama.cpp/blob/5c0eb5ef544aeefd81c303e03208f768e158d93c/tools/main/README.md#mirostat-sampling"/>
/// <br/>• <see href="https://github.com/ggml-org/llama.cpp/blob/5c0eb5ef544aeefd81c303e03208f768e158d93c/tools/server/README.md"/>
/// <br/>• <see href="https://kojix2.github.io/llama.cr/Llama/Sampler/MirostatV2.html"/>
/// </remarks>
public sealed class MirostatV2Config : IMethodConfig
{
    private readonly Resource _samplerResource;

    /// <summary>
    /// Constructs a new instance of the <see cref="MirostatV2Config"/>.
    /// </summary>
    /// <param name="resource">The actual sampler resource from the GDExtension.</param>
    public MirostatV2Config(Resource resource)
    {
        ArgumentNullException.ThrowIfNull(resource);

        if(!resource.IsClass(nameof(NobodyWhoSampler)))
        {
            throw new ArgumentException($"Resource must be of class {nameof(NobodyWhoSampler)}", nameof(resource));
        }

        _samplerResource = resource;
    }

    /// <inheritdoc/>
    public SamplerMethod Method => SamplerMethod.MirostatV2;

    /// <summary>
    /// The option that controls the Mirostat learning rate. The learning rate influences how quickly the algorithm responds to feedback from the generated text.
    /// A lower learning rate will result in slower adjustments, while a higher learning rate will make the algorithm more responsive.
    /// Defaults to <strong>0.1</strong>.
    /// </summary>
    public float Eta
    {
        get => _samplerResource.Get(PropertyName.Eta).As<float>();
        set => _samplerResource.Set(PropertyName.Eta, value);
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
    /// The option that controls the Mirostat target entropy, which represents the desired perplexity value for the generated text.
    /// Adjusting the target entropy allows you to control the balance between coherence and diversity in the generated text.
    /// A lower value will result in more focused and coherent text, while a higher value will lead to more diverse and potentially less coherent text.
    /// Defaults to <strong>5.0</strong>.
    /// </summary>
    public float Tau
    {
        get => _samplerResource.Get(PropertyName.Tau).As<float>();
        set => _samplerResource.Set(PropertyName.Tau, value);
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
        /// <strong>eta</strong>
        /// </summary>
        public static readonly StringName Eta = "eta";

        /// <summary>
        /// <strong>seed</strong>
        /// </summary>
        public static readonly StringName Seed = "seed";

        /// <summary>
        /// <strong>tau</strong>
        /// </summary>
        public static readonly StringName Tau = "tau";

        /// <summary>
        /// <strong>temperature</strong>
        /// </summary>
        public static readonly StringName Temperature = "temperature";
    }

    #endregion Names
}