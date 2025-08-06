using System;
using NobodyWho.Enums;

namespace NobodyWho.Models.SamplerConfigs;

/// <summary>
/// The interface that defines a method config class that will contain the values for the associated sampler method.
/// </summary>
public interface IMethodConfig
{
    /// <summary>
    /// The associated sampler method for the config.
    /// </summary>
    SamplerMethod Method { get; }

    /// <summary>
    /// Casts the current implementation type of <see cref="IMethodConfig"/> into the <typeparamref name="T"/> type.
    /// </summary>
    /// <typeparam name="T">The implementation type of <see cref="IMethodConfig"/> to cast into.</typeparam>
    /// <returns>The <typeparamref name="T"/> that represents the casted instance of a <see cref="IMethodConfig"/> implementation.</returns>
    /// <exception cref="InvalidCastException"></exception>
    public T As<T>() where T : class, IMethodConfig
    {
        if(this is T config)
        {
            return config;
        }

        throw new InvalidCastException($"Cannot cast {GetType().Name} into {typeof(T).Name}.");
    }
}