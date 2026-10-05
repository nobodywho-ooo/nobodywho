using System;
using System.Collections.Generic;
using System.Linq;

namespace NobodyWho;

/// <summary>Context usage of a chat.</summary>
/// <param name="ContextSize">The chat's context size, in tokens.</param>
/// <param name="ContextUsed">Tokens currently in the context.</param>
public sealed record ChatStats(int ContextSize, int ContextUsed);

/// <summary>A cached <c>.gguf</c> model and its on-disk size in bytes.</summary>
public sealed record CachedModel(string Path, long Size);

/// <summary>Bytes downloaded so far, out of the total, while a remote model downloads.</summary>
public readonly record struct DownloadProgress(long Downloaded, long Total);

/// <summary>
/// Tuning for MTP speculative decoding. Passing one to <see cref="Chat"/> enables MTP.
/// Requires the model to have been loaded with a compatible draft model.
/// </summary>
/// <param name="KMax">
/// Maximum draft tokens proposed per speculative step. Higher values draft more per decode;
/// returns diminish past about 4 to 6.
/// </param>
/// <param name="PMin">
/// Minimum draft-token probability the drafter will propose. <c>0</c> accepts all proposals;
/// raise it to skip low-confidence drafts.
/// </param>
public sealed record MtpConfig(int KMax = 3, float PMin = 0f)
{
    internal Native.MtpConfig ToNative() => new(Checked.ToUInt(KMax, nameof(KMax)), PMin);
}

/// <summary>Size a context shift shrinks the chat history to.</summary>
public abstract record ShiftTarget
{
    private ShiftTarget() { }

    /// <summary>A fraction of the context size, in (0, 1).</summary>
    public sealed record Fraction(float Value) : ShiftTarget;

    /// <summary>A number of tokens, below the context size.</summary>
    public sealed record Tokens(int Value) : ShiftTarget;

    internal Native.ShiftTarget ToNative() => this switch
    {
        Fraction f => new Native.ShiftTarget.Fraction(f.Value),
        Tokens t => new Native.ShiftTarget.Tokens(Checked.ToUInt(t.Value, nameof(Tokens))),
        _ => throw new InvalidOperationException($"Unknown shift target {GetType()}"),
    };
}

/// <summary>
/// How a chat forgets old turns when its context is full. A turn is a user message and
/// everything up to the next one; system messages are always kept.
/// </summary>
/// <param name="Enabled"><c>false</c> disables shifting, so a full context is an error instead.</param>
/// <param name="KeepFirstTurns">Turns always kept at the start of the history.</param>
/// <param name="KeepLastTurns">Turns always kept at the end of the history; at least 1.</param>
/// <param name="Target">Size the history is shrunk to. <c>null</c> means half the context size.</param>
public sealed record ContextShiftOptions(
    bool Enabled = true,
    int KeepFirstTurns = 1,
    int KeepLastTurns = 2,
    ShiftTarget? Target = null)
{
    internal Native.ContextShiftOptions ToNative() => new(
        Enabled,
        Checked.ToUInt(KeepFirstTurns, nameof(KeepFirstTurns)),
        Checked.ToUInt(KeepLastTurns, nameof(KeepLastTurns)),
        Target?.ToNative());
}

/// <summary>
/// Settings to apply before a <see cref="Chat.Complete"/> turn. A <c>null</c> field keeps what
/// the chat has; a set one stays set, like a leading system message.
/// </summary>
/// <param name="Sampler">The sampler to use from this turn on.</param>
/// <param name="TemplateVariables">Replaces the chat's template variables wholesale.</param>
/// <param name="Tools">
/// Re-selects the chat template, so the turn re-prefills from near token zero. An empty list
/// removes the tools.
/// </param>
public sealed record CompletionOptions(
    SamplerConfig? Sampler = null,
    IReadOnlyDictionary<string, bool>? TemplateVariables = null,
    IReadOnlyList<Tool>? Tools = null)
{
    internal Native.Options ToNative() => new(
        Sampler?.Inner,
        TemplateVariables?.ToDictionary(kv => kv.Key, kv => kv.Value),
        Tools?.Select(t => t.Inner).ToArray());
}

/// <summary>An error reported by NobodyWho's native library.</summary>
public sealed class NobodyWhoException : Exception
{
    /// <summary>Create an exception with the given message.</summary>
    public NobodyWhoException(string message) : base(message) { }

    /// <summary>Create an exception with the given message and cause.</summary>
    public NobodyWhoException(string message, Exception innerException) : base(message, innerException) { }
}

/// <summary>Helpers for crossing into the generated layer.</summary>
internal static class Checked
{
    internal static uint ToUInt(int value, string name) =>
        value >= 0 ? (uint)value : throw new ArgumentOutOfRangeException(name, value, "Must not be negative.");

    internal static uint? ToUInt(int? value, string name) => value is { } v ? ToUInt(v, name) : null;

    /// <summary>Run a native call, turning its errors into <see cref="NobodyWhoException"/>.</summary>
    internal static T Call<T>(Func<T> call)
    {
        try
        {
            return call();
        }
        catch (Native.NobodyWhoException e)
        {
            throw Wrap(e);
        }
    }

    /// <inheritdoc cref="Call{T}(Func{T})"/>
    internal static void Call(Action call)
    {
        try
        {
            call();
        }
        catch (Native.NobodyWhoException e)
        {
            throw Wrap(e);
        }
    }

    /// <inheritdoc cref="Call{T}(Func{T})"/>
    internal static async System.Threading.Tasks.Task<T> CallAsync<T>(Func<System.Threading.Tasks.Task<T>> call)
    {
        try
        {
            return await WithoutContext(call).ConfigureAwait(false);
        }
        catch (Native.NobodyWhoException e)
        {
            throw Wrap(e);
        }
    }

    /// <inheritdoc cref="Call{T}(Func{T})"/>
    internal static async System.Threading.Tasks.Task CallAsync(Func<System.Threading.Tasks.Task> call)
    {
        try
        {
            await WithoutContext(call).ConfigureAwait(false);
        }
        catch (Native.NobodyWhoException e)
        {
            throw Wrap(e);
        }
    }

    /// <summary>
    /// Start a generated async call with no <see cref="System.Threading.SynchronizationContext"/>.
    /// The generated code awaits without <c>ConfigureAwait(false)</c>, so on a UI thread it would
    /// otherwise resume there, and deadlock any caller that blocks on the task.
    /// </summary>
    private static TTask WithoutContext<TTask>(Func<TTask> call)
    {
        var context = System.Threading.SynchronizationContext.Current;
        System.Threading.SynchronizationContext.SetSynchronizationContext(null);
        try
        {
            return call();
        }
        finally
        {
            System.Threading.SynchronizationContext.SetSynchronizationContext(context);
        }
    }

    private static NobodyWhoException Wrap(Native.NobodyWhoException e) =>
        new(e.Message, e);
}
