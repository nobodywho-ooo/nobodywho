using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;

namespace NobodyWho;

/// <summary>
/// A loaded GGUF model. One model can be shared by any number of <see cref="Chat"/>,
/// <see cref="Encoder"/> and <see cref="CrossEncoder"/> instances.
/// </summary>
/// <example>
/// <code>
/// using var model = await Model.LoadAsync("hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf");
/// using var chat = new Chat(model, systemPrompt: "You are a helpful assistant.");
/// </code>
/// </example>
public sealed class Model : IDisposable
{
    internal Native.RustModel Inner { get; }

    private Model(Native.RustModel inner) => Inner = inner;

    /// <summary>Load a GGUF model from a local path or a remote URL.</summary>
    /// <param name="modelPath">
    /// Path to a <c>.gguf</c> file, an <c>hf://owner/repo/file.gguf</c> or <c>https://</c> URL,
    /// or <c>auto</c> to pick a model that fits in memory. Downloads are cached.
    /// </param>
    /// <param name="useGpu">Use GPU acceleration when available.</param>
    /// <param name="projectionModelPath">Path to an <c>mmproj</c> file, for vision and audio models.</param>
    /// <param name="draftModelPath">
    /// Path to MTP draft heads. Loading them lets chats on this model opt into MTP speculative
    /// decoding with <see cref="MtpConfig"/>. Adds around 5% to VRAM usage.
    /// </param>
    /// <param name="progress">Receives download progress, if the model has to be downloaded.</param>
    /// <param name="cancellationToken">
    /// Stops waiting for the load. The native load keeps running in the background and its
    /// result is freed when it finishes.
    /// </param>
    /// <exception cref="NobodyWhoException">The model could not be found, downloaded or loaded.</exception>
    public static async Task<Model> LoadAsync(
        string modelPath,
        bool useGpu = true,
        string? projectionModelPath = null,
        string? draftModelPath = null,
        IProgress<DownloadProgress>? progress = null,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(modelPath);
        var load = Checked.CallAsync(() => Native.NativeMethods.LoadModel(
            modelPath, useGpu, projectionModelPath, draftModelPath, ProgressCallback.From(progress)));
        return new Model(await Cancellation.WaitOrDispose(load, cancellationToken).ConfigureAwait(false));
    }

    /// <summary>
    /// Download a model from a remote URL or Hugging Face path and return the local file path.
    /// </summary>
    /// <remarks>
    /// Use this when you need custom headers, such as for gated models that require
    /// authentication. For other downloads, pass the URL straight to <see cref="LoadAsync"/>.
    /// </remarks>
    /// <param name="modelPath">An <c>hf://owner/repo/file.gguf</c> path or an <c>https://</c> URL.</param>
    /// <param name="headers">HTTP headers to send, such as <c>Authorization</c>.</param>
    /// <param name="progress">Receives download progress.</param>
    /// <param name="cancellationToken">Stops waiting for the download. The download itself keeps running.</param>
    public static Task<string> DownloadAsync(
        string modelPath,
        IReadOnlyDictionary<string, string>? headers = null,
        IProgress<DownloadProgress>? progress = null,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(modelPath);
        return Checked.CallAsync(() => Native.NativeMethods.DownloadModel(
                modelPath,
                headers?.ToDictionary(kv => kv.Key, kv => kv.Value),
                ProgressCallback.From(progress)))
            .WaitAsync(cancellationToken);
    }

    /// <summary>Every cached <c>.gguf</c> model, with its size on disk.</summary>
    public static IReadOnlyList<CachedModel> GetCachedModels() =>
        Checked.Call(Native.NativeMethods.GetCachedModels)
            .Select(m => new CachedModel(m.Path, checked((long)m.Size)))
            .ToArray();

    /// <summary>The identifier this model was loaded from.</summary>
    public string Source => Inner.Source();

    /// <summary>
    /// The context size this model was trained with. Setting a chat's context size above this
    /// gives poor results.
    /// </summary>
    public int MaxContextSize => checked((int)Inner.MaxCtx());

    /// <summary>
    /// Free the underlying native model. Chats, encoders and cross-encoders created from it keep
    /// their own reference and stay usable.
    /// </summary>
    public void Dispose() => Inner.Dispose();

    private sealed class ProgressCallback(IProgress<DownloadProgress> progress) : Native.RustDownloadProgressCallback
    {
        public static ProgressCallback? From(IProgress<DownloadProgress>? progress) =>
            progress is null ? null : new ProgressCallback(progress);

        public void OnDownloadProgress(ulong downloaded, ulong total)
        {
            // Never let an exception unwind into native code.
            try
            {
                progress.Report(new DownloadProgress((long)downloaded, (long)total));
            }
            catch
            {
                // A failing progress handler must not abort the download.
            }
        }
    }
}

/// <summary>Cancellation for native calls that cannot be cancelled themselves.</summary>
internal static class Cancellation
{
    /// <summary>
    /// Wait for <paramref name="task"/>, giving up when <paramref name="cancellationToken"/> fires.
    /// A result that arrives after the wait was given up is disposed rather than leaked.
    /// </summary>
    internal static async Task<T> WaitOrDispose<T>(Task<T> task, CancellationToken cancellationToken)
        where T : IDisposable
    {
        try
        {
            return await task.WaitAsync(cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            _ = task.ContinueWith(
                t => t.Result.Dispose(),
                CancellationToken.None,
                TaskContinuationOptions.OnlyOnRanToCompletion | TaskContinuationOptions.ExecuteSynchronously,
                TaskScheduler.Default);
            throw;
        }
    }
}
