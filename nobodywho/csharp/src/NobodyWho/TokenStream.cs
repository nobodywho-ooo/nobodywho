using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;

namespace NobodyWho;

/// <summary>
/// The tokens of a response, as the model generates them.
/// </summary>
/// <remarks>
/// Enumerate it with <c>await foreach</c> to stream tokens, or call <see cref="CompletedAsync"/>
/// for the whole response. Cancelling either stops generation, and the chat stays usable.
/// </remarks>
/// <example>
/// <code>
/// await foreach (var token in chat.Ask("Is water wet?"))
///     Console.Write(token);
///
/// string response = await chat.Ask("Is water wet?").CompletedAsync();
/// </code>
/// </example>
public sealed class TokenStream : IAsyncEnumerable<string>, IDisposable
{
    private readonly Native.RustTokenStream _inner;
    private readonly Action _stop;

    internal TokenStream(Native.RustTokenStream inner, Action stop)
    {
        _inner = inner;
        _stop = () =>
        {
            try
            {
                stop();
            }
            catch (ObjectDisposedException)
            {
                // The chat is gone, so nothing is generating.
            }
        };
    }

    /// <summary>The next token, or <c>null</c> once the response is complete.</summary>
    /// <param name="cancellationToken">Stops generation.</param>
    /// <exception cref="NobodyWhoException">Generation failed.</exception>
    public Task<string?> NextTokenAsync(CancellationToken cancellationToken = default) =>
        StoppingOnCancel(_inner.NextToken, cancellationToken);

    /// <summary>Wait for the response to finish and return all of it.</summary>
    /// <param name="cancellationToken">Stops generation.</param>
    /// <exception cref="NobodyWhoException">Generation failed.</exception>
    public Task<string> CompletedAsync(CancellationToken cancellationToken = default) =>
        StoppingOnCancel(_inner.Completed, cancellationToken);

    /// <summary>
    /// Run a native call, stopping generation when <paramref name="cancellationToken"/> fires and
    /// then throwing <see cref="OperationCanceledException"/>.
    /// </summary>
    private async Task<T> StoppingOnCancel<T>(Func<Task<T>> call, CancellationToken cancellationToken)
    {
        var task = Checked.CallAsync(call);
        if (cancellationToken.CanBeCanceled)
        {
            try
            {
                await task.WaitAsync(cancellationToken).ConfigureAwait(false);
            }
            catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
            {
                // The worker clears a stop when it starts on a request, so a stop sent while this
                // response was still queued is lost. Keep stopping until the call returns.
                while (!task.IsCompleted)
                {
                    _stop();
                    await Task.WhenAny(task, Task.Delay(50, CancellationToken.None)).ConfigureAwait(false);
                }
            }
        }
        var result = await task.ConfigureAwait(false);
        cancellationToken.ThrowIfCancellationRequested();
        return result;
    }

    /// <inheritdoc/>
    public async IAsyncEnumerator<string> GetAsyncEnumerator(CancellationToken cancellationToken = default)
    {
        while (await NextTokenAsync(cancellationToken).ConfigureAwait(false) is { } token)
            yield return token;
    }

    /// <summary>Free the underlying native resources.</summary>
    public void Dispose() => _inner.Dispose();
}

/// <summary>
/// The tokens of a transcript, as <see cref="SpeechToText"/> produces them.
/// </summary>
/// <example>
/// <code>
/// await foreach (var token in stt.TranscribeFile("recording.mp3"))
///     Console.Write(token);
/// </code>
/// </example>
public sealed class SpeechToTextStream : IAsyncEnumerable<string>, IDisposable
{
    private readonly Native.RustSpeechToTextStream _inner;

    internal SpeechToTextStream(Native.RustSpeechToTextStream inner) => _inner = inner;

    /// <summary>The next transcript token, or <c>null</c> once transcription is complete.</summary>
    /// <param name="cancellationToken">Stops waiting. Transcription keeps running in the background.</param>
    public Task<string?> NextTokenAsync(CancellationToken cancellationToken = default) =>
        Checked.CallAsync(_inner.NextToken).WaitAsync(cancellationToken);

    /// <summary>Wait for transcription to finish and return the full transcript.</summary>
    /// <param name="cancellationToken">Stops waiting. Transcription keeps running in the background.</param>
    public Task<string> CompletedAsync(CancellationToken cancellationToken = default) =>
        Checked.CallAsync(_inner.Completed).WaitAsync(cancellationToken);

    /// <inheritdoc/>
    public async IAsyncEnumerator<string> GetAsyncEnumerator(CancellationToken cancellationToken = default)
    {
        while (await NextTokenAsync(cancellationToken).ConfigureAwait(false) is { } token)
            yield return token;
    }

    /// <summary>Free the underlying native resources.</summary>
    public void Dispose() => _inner.Dispose();
}
