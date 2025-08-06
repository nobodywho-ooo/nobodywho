using System;
using System.Threading;
using System.Threading.Tasks;
using Godot;
using Polly;
using Polly.Timeout;

namespace NobodyWho.Extensions;

/// <summary>
/// Extension functions for the <see cref="GodotObject"/> type.
/// </summary>
public static class GodotObjectExtensions
{
    /// <summary>
    /// Creates and awaits the result of a new <see cref="SignalAwaiter"/> awaiter configured to complete when the provided <paramref name="source"/> emits
    /// the signal specified by the provided <paramref name="signal"/>.
    /// </summary>
    /// <remarks>
    /// <strong>NOTE:</strong> This is used as a one-to-one replacement for <see cref="GodotObject.ToSignal(GodotObject, StringName)"/> and awaiting the
    /// result, with timeout and cancellation support.
    /// </remarks>
    /// <param name="target">The object used as the awaiter target (typically <see langword="this"/>).</param>
    /// <param name="source">The instance the awaiter will be listening to.</param>
    /// <param name="signal">The name of the signal the awaiter will be waiting for.</param>
    /// <param name="timeout">The amount of time to wait before timing out.</param>
    /// <param name="cancellationToken">The optional cancellation token which can be used to cancel the action.</param>
    /// <returns>The <see cref="Task"/> that represents the asynchronous operation, containing the collection of <see cref="Variant"/> objects.
    /// Returns <strong>null</strong> if the operation times out (after the provided <paramref name="timeout"/>) or is canceled via the provided <paramref name="cancellationToken"/>.</returns>
    public static async Task<Variant[]?> AwaitSignalAsync(this GodotObject target,
        GodotObject source,
        StringName signal,
        TimeSpan timeout,
        CancellationToken cancellationToken = default)
    {
        AsyncTimeoutPolicy<Variant[]> timeoutPolicy =
            Policy.TimeoutAsync<Variant[]>(timeout, TimeoutStrategy.Pessimistic);

        SignalAwaiter signalAwaiter = new(source, signal, target);

        try
        {
            return await timeoutPolicy.ExecuteAsync(async ct =>
            {
                return await signalAwaiter;
            }, cancellationToken);
        }
        catch(TimeoutRejectedException)
        {
            GD.PushError($"Timed out awaiting for the `{signal}` signal to emit.");

            return null;
        }
        catch(OperationCanceledException)
        {
            GD.PushError($"Operation canceled awaiting for the `{signal}` signal by the cancellation token.");

            return null;
        }
    }

    /// <summary>
    /// Calls the method specified by provided <paramref name="method"/> name on the provided <paramref name="target"/>, which is expected to return an awaitable <see cref="Signal"/>, and awaits the result.
    /// </summary>
    /// <remarks>
    /// <strong>NOTE:</strong> This is used as a one-to-one replacement for <see cref="GodotObject.Call(StringName, Variant[])"/> when the method returns an awaitable <see cref="Signal"/>,
    /// with timeout and cancellation support.
    /// </remarks>
    /// <param name="target">The object to call the method on.</param>
    /// <param name="method">The method to be called.</param>
    /// <param name="timeout">The amount of time to wait before timing out.</param>
    /// <param name="cancellationToken">The optional cancellation token which can be used to cancel the action.</param>
    /// <param name="args">The argument(s), if any, to pass into the method to call.</param>
    /// <returns>The <see cref="Task"/> that represents the asynchronous operation, containing the collection of <see cref="Variant"/> objects.
    /// Returns <strong>null</strong> if the operation times out (after the provided <paramref name="timeout"/>) or is canceled via the provided <paramref name="cancellationToken"/>.</returns>
    public static async Task<Variant[]?> AwaitCallAsync(this GodotObject target,
        StringName method,
        TimeSpan timeout,
        CancellationToken cancellationToken = default,
        params Variant[] args)
    {
        AsyncTimeoutPolicy<Variant[]> timeoutPolicy =
            Policy.TimeoutAsync<Variant[]>(timeout, TimeoutStrategy.Pessimistic);

        try
        {
            Signal signal = target.Call(method, args).AsSignal();

            return await timeoutPolicy.ExecuteAsync(async ct =>
            {
                return await signal;
            }, cancellationToken);
        }
        catch(TimeoutRejectedException)
        {
            GD.PushError($"Timed out awaiting the returned signal of the called method `{method}`.");

            return null;
        }
        catch(OperationCanceledException)
        {
            GD.PushError($"Operation canceled awaiting for the `{method}` method by the cancellation token.");

            return null;
        }
        catch(Exception ex)
        {
            GD.PushError($"An unknown error occurred while trying to perform the {nameof(AwaitCallAsync)}. Error details:\n{ex.Message}");

            return null;
        }
    }
}