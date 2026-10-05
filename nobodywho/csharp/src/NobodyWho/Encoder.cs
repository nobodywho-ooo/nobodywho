using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;

namespace NobodyWho;

/// <summary>
/// Turns text into embedding vectors, for semantic search and retrieval. Use an embedding model.
/// </summary>
/// <example>
/// <code>
/// using var encoder = await Encoder.FromPathAsync("embeddings.gguf");
/// float[] a = await encoder.EncodeAsync("Copenhagen is in Denmark.");
/// float[] b = await encoder.EncodeAsync("Denmark's capital is Copenhagen.");
/// float similarity = Encoder.CosineSimilarity(a, b);
/// </code>
/// </example>
public sealed class Encoder : IDisposable
{
    private readonly Native.RustEncoder _inner;
    private readonly Model? _ownedModel;

    /// <summary>Create an encoder on a loaded embedding model.</summary>
    /// <param name="model">The embedding model.</param>
    /// <param name="contextSize">The context size, in tokens. <c>null</c> uses 4096.</param>
    public Encoder(Model model, int? contextSize = null) : this(model, null, contextSize) { }

    private Encoder(Model model, Model? ownedModel, int? contextSize)
    {
        ArgumentNullException.ThrowIfNull(model);
        _inner = new Native.RustEncoder(model.Inner, Checked.ToUInt(contextSize, nameof(contextSize)));
        _ownedModel = ownedModel;
    }

    /// <summary>
    /// Load a model and create an encoder on it in one step. The encoder owns the model and frees
    /// it when disposed.
    /// </summary>
    public static async Task<Encoder> FromPathAsync(
        string modelPath,
        bool useGpu = true,
        int? contextSize = null,
        IProgress<DownloadProgress>? progress = null,
        CancellationToken cancellationToken = default)
    {
        Checked.ToUInt(contextSize, nameof(contextSize));
        var model = await Model.LoadAsync(modelPath, useGpu, progress: progress, cancellationToken: cancellationToken)
            .ConfigureAwait(false);
        try
        {
            return new Encoder(model, model, contextSize);
        }
        catch
        {
            model.Dispose();
            throw;
        }
    }

    /// <summary>Embed one text.</summary>
    public Task<float[]> EncodeAsync(string text) => Checked.CallAsync(() => _inner.Encode(text));

    /// <summary>Embed several texts. The vectors come back in the same order as the texts.</summary>
    public Task<float[][]> EncodeBatchAsync(IEnumerable<string> texts)
    {
        ArgumentNullException.ThrowIfNull(texts);
        var array = texts.ToArray();
        return Checked.CallAsync(() => _inner.EncodeBatch(array));
    }

    /// <summary>The cosine similarity of two vectors, in [-1, 1].</summary>
    public static float CosineSimilarity(float[] a, float[] b)
    {
        ArgumentNullException.ThrowIfNull(a);
        ArgumentNullException.ThrowIfNull(b);
        return Native.NativeMethods.CosineSimilarity(a, b);
    }

    /// <summary>Free the underlying native resources, and the model if this encoder owns it.</summary>
    public void Dispose()
    {
        _inner.Dispose();
        _ownedModel?.Dispose();
    }
}

/// <summary>
/// Ranks documents by how relevant they are to a query. Use a reranker (cross-encoder) model.
/// </summary>
/// <example>
/// <code>
/// using var reranker = await CrossEncoder.FromPathAsync("reranker.gguf");
/// var ranked = await reranker.RankAndSortAsync("What is the capital of Denmark?", documents);
/// foreach (var (document, score) in ranked)
///     Console.WriteLine($"{score:F3} {document}");
/// </code>
/// </example>
public sealed class CrossEncoder : IDisposable
{
    private readonly Native.RustCrossEncoder _inner;
    private readonly Model? _ownedModel;

    /// <summary>Create a cross-encoder on a loaded reranker model.</summary>
    /// <param name="model">The reranker model.</param>
    /// <param name="contextSize">The context size, in tokens. <c>null</c> uses 4096.</param>
    public CrossEncoder(Model model, int? contextSize = null) : this(model, null, contextSize) { }

    private CrossEncoder(Model model, Model? ownedModel, int? contextSize)
    {
        ArgumentNullException.ThrowIfNull(model);
        _inner = new Native.RustCrossEncoder(model.Inner, Checked.ToUInt(contextSize, nameof(contextSize)));
        _ownedModel = ownedModel;
    }

    /// <summary>
    /// Load a model and create a cross-encoder on it in one step. The cross-encoder owns the
    /// model and frees it when disposed.
    /// </summary>
    public static async Task<CrossEncoder> FromPathAsync(
        string modelPath,
        bool useGpu = true,
        int? contextSize = null,
        IProgress<DownloadProgress>? progress = null,
        CancellationToken cancellationToken = default)
    {
        Checked.ToUInt(contextSize, nameof(contextSize));
        var model = await Model.LoadAsync(modelPath, useGpu, progress: progress, cancellationToken: cancellationToken)
            .ConfigureAwait(false);
        try
        {
            return new CrossEncoder(model, model, contextSize);
        }
        catch
        {
            model.Dispose();
            throw;
        }
    }

    /// <summary>Score each document's relevance to the query. Scores come back in document order.</summary>
    public Task<float[]> RankAsync(string query, IEnumerable<string> documents)
    {
        ArgumentNullException.ThrowIfNull(documents);
        var array = documents.ToArray();
        return Checked.CallAsync(() => _inner.Rank(query, array));
    }

    /// <summary>The documents with their scores, most relevant first.</summary>
    public async Task<IReadOnlyList<(string Document, float Score)>> RankAndSortAsync(string query, IEnumerable<string> documents)
    {
        ArgumentNullException.ThrowIfNull(documents);
        var array = documents.ToArray();
        var json = await Checked.CallAsync(() => _inner.RankAndSortJson(query, array)).ConfigureAwait(false);
        using var doc = JsonDocument.Parse(json);
        return doc.RootElement.EnumerateArray()
            .Select(pair => (pair[0].GetString()!, pair[1].GetSingle()))
            .ToArray();
    }

    /// <summary>Free the underlying native resources, and the model if this cross-encoder owns it.</summary>
    public void Dispose()
    {
        _inner.Dispose();
        _ownedModel?.Dispose();
    }
}
