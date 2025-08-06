using System;
using Godot;
using NobodyWho.Enums;
using NobodyWho.Models.SamplerConfigs;

namespace NobodyWho;

/// <summary>
/// <para>The wrapper class for the GDExtension <c>NobodyWhoSampler</c> <see cref="Resource"/>, which shares the same name.</para>
/// <para><strong><c>NobodyWhoSampler</c> is the main way to configure the generation strategies and penalties of the LLM.</strong></para>
/// </summary>
public sealed class NobodyWhoSampler
{
    private IMethodConfig? _methodConfig;

    /// <summary>
    /// Constructs a new instance of the <see cref="NobodyWhoSampler"/>.
    /// </summary>
    /// <param name="resource">The actual sampler resource from the GDExtension.</param>
    /// <exception cref="ArgumentException"></exception>
    /// <exception cref="ArgumentNullException"></exception>
    public NobodyWhoSampler(Resource resource)
    {
        ArgumentNullException.ThrowIfNull(resource);

        if(!resource.IsClass(nameof(NobodyWhoSampler)))
        {
            throw new ArgumentException($"Resource must be of class {nameof(NobodyWhoSampler)}", nameof(resource));
        }

        SamplerResource = resource;
    }

    #region Properties

    /// <summary>
    /// The actual instance of the GDExtension <c>NobodyWhoSampler</c> resource.
    /// </summary>
    public Resource SamplerResource { get; init; }

    /// <summary>
    /// The option to control the set sampling method to use for token generation.
    /// Defaults to <see cref="SamplerMethod.MirostatV2"/>.
    /// </summary>
    public SamplerMethod Method
    {
        get
        {
            string methodName = SamplerResource.Get(PropertyName.Method).AsString();

            if(Enum.TryParse(methodName, ignoreCase: true, out SamplerMethod method))
            {
                return method;
            }

            throw new InvalidOperationException($"Invalid sampler method name `{methodName}` on the {nameof(SamplerResource)}");
        }
        set
        {
            if(Method != value)
            {
                _methodConfig = null;
                SamplerResource.Set(PropertyName.Method, value.ToString());
            }
        }
    }

    /// <summary>
    /// The options for the current <see cref="Method"/>.
    /// </summary>
    public IMethodConfig MethodConfig
    {
        get
        {
            if(_methodConfig is not null)
            {
                return _methodConfig;
            }

            _methodConfig = Method switch
            {
                SamplerMethod.DRY => new DryConfig(SamplerResource),
                SamplerMethod.Greedy => new GreedyConfig(),
                SamplerMethod.MinP => new MinPConfig(SamplerResource),
                SamplerMethod.MirostatV1 => new MirostatV1Config(SamplerResource),
                SamplerMethod.MirostatV2 => new MirostatV2Config(SamplerResource),
                SamplerMethod.Temperature => new TemperatureConfig(SamplerResource),
                SamplerMethod.TopK => new TopKConfig(SamplerResource),
                SamplerMethod.TopP => new TopPConfig(SamplerResource),
                SamplerMethod.TypicalP => new TypicalPConfig(SamplerResource),
                SamplerMethod.XTC => new XTCConfig(SamplerResource),
                _ => throw new InvalidOperationException("Unknown sampler method")
            };

            return _methodConfig;
        }
    }

    /// <summary>
    /// The option to control the repeat alpha frequency penalty.
    /// Disabled when set as <strong>0.0</strong>.
    /// Defaults to <strong>0.0</strong>.
    /// </summary>
    public float PenaltyFrequency
    {
        get
        {
            return SamplerResource.Get(PropertyName.PenaltyFrequency).As<float>();
        }
        set
        {
            SamplerResource.Set(PropertyName.PenaltyFrequency, value);
        }
    }

    /// <summary>
    /// The option to control the number of tokens in the history to consider for penalizing repetition.
    /// A larger value will look further back in the generated text to prevent repetitions, while a smaller value will only consider recent tokens.
    /// Disabled when set as <strong>0</strong>. Uses the entire context size when set as <strong>-1</strong>.
    /// Defaults to <strong>-1</strong>.
    /// </summary>
    public int PenaltyLastN
    {
        get
        {
            return SamplerResource.Get(PropertyName.PenaltyLastN).AsInt32();
        }
        set
        {
            SamplerResource.Set(PropertyName.PenaltyLastN, value);
        }
    }

    /// <summary>
    /// The option to control the repeat alpha presence penalty.
    /// Disabled when set as <strong>0.0</strong>.
    /// Defaults to <strong>0.0</strong>.
    /// </summary>
    public float PenaltyPresent
    {
        get
        {
            return SamplerResource.Get(PropertyName.PenaltyPresent).As<float>();
        }
        set
        {
            SamplerResource.Set(PropertyName.PenaltyPresent, value);
        }
    }

    /// <summary>
    /// The option to control the repetition of token sequences in the generated text, which helps to prevent the model from generating repetitive or monotonous text.
    /// A higher value <strong>(e.g., 1.5)</strong> will penalize repetitions more strongly, while a lower value <strong>(e.g., 0.9)</strong> will be more lenient.
    /// Disabled when set as <strong>1.0</strong>.
    /// Defaults to <strong>0.0</strong>.
    /// </summary>
    public float PenaltyRepeat
    {
        get
        {
            return SamplerResource.Get(PropertyName.PenaltyRepeat).As<float>();
        }
        set
        {
            SamplerResource.Set(PropertyName.PenaltyRepeat, value);
        }
    }

    /// <summary>
    /// The option to control whether the sampler should use grammar or not.
    /// Defaults to <see langword="false"/>.
    /// </summary>
    public bool UseGrammar
    {
        get
        {
            return SamplerResource.Get(PropertyName.UseGrammar).AsBool();
        }
        set
        {
            SamplerResource.Set(PropertyName.UseGrammar, value);
        }
    }

    /// <summary>
    /// The option to control the grammar definition used by the sampler.
    /// Uses the following format: GBNF (Grammar-Based Neural Format)
    /// </summary>
    public string GbnfGrammar
    {
        get
        {
            return SamplerResource.Get(PropertyName.GbnfGrammar).AsString();
        }
        set
        {
            SamplerResource.Set(PropertyName.GbnfGrammar, value);
        }
    }

    #endregion Properties

    #region Factory

    /// <summary>
    /// Creates a new instance of the <see cref="NobodyWhoSampler"/> wrapper and creates the inner GDExtension sampler resource.
    /// </summary>
    /// <returns>The <see cref="NobodyWhoSampler"/> that represents the wrapper of the newly created inner GDExtension sampler resource.</returns>
    /// <exception cref="InvalidOperationException"></exception>
    public static NobodyWhoSampler Create()
    {
        Resource? samplerResource = (Resource?) ClassDB.Instantiate(nameof(NobodyWhoSampler))
            ?? throw new InvalidOperationException($"Failed to instantiate GDExtension class {nameof(NobodyWhoSampler)}.");

        return new(samplerResource);
    }

    #endregion Factory

    #region Names

    /// <summary>
    /// Cached StringNames for the properties and fields contained in this class, for fast lookup.
    /// </summary>
    private static class PropertyName
    {
        /// <summary>
        /// <strong>method</strong>
        /// </summary>
        public static readonly StringName Method = "method";

        /// <summary>
        /// <strong>penalty_freq</strong>
        /// </summary>
        public static readonly StringName PenaltyFrequency = "penalty_freq";

        /// <summary>
        /// <strong>penalty_last_n</strong>
        /// </summary>
        public static readonly StringName PenaltyLastN = "penalty_last_n";

        /// <summary>
        /// <strong>penalty_present</strong>
        /// </summary>
        public static readonly StringName PenaltyPresent = "penalty_present";

        /// <summary>
        /// <strong>penalty_repeat</strong>
        /// </summary>
        public static readonly StringName PenaltyRepeat = "penalty_repeat";

        /// <summary>
        /// <strong>use_grammar</strong>
        /// </summary>
        public static readonly StringName UseGrammar = "use_grammar";

        /// <summary>
        /// <strong>gbnf_grammar</strong>
        /// </summary>
        public static readonly StringName GbnfGrammar = "gbnf_grammar";
    }

    #endregion Names
}