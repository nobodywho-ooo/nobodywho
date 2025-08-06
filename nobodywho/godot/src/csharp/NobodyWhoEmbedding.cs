using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Godot;
using NobodyWho.Enums;
using NobodyWho.Extensions;

namespace NobodyWho;

/// <summary>
/// <para>The wrapper class for the GDExtension <c>NobodyWhoEmbedding</c> <see cref="Node"/>, which shares the same name.</para>
/// <para><strong>The Embedding node is used to compare text.
/// This is useful for detecting whether the user said something specific, without having to match on literal keywords or sentences.
/// </strong></para>
/// <para>This is done by embedding the text into a vector space and then comparing the cosine similarity between the vectors.</para>
/// <para>A good example of this would be to check if a user signals an action like "I'd like to buy the red potion". The following sentences will have high similarity:<br/>
/// • Give me the potion that is red<br/>
/// • I'd like the red one, please.<br/>
/// • Hand me the flask of scarlet hue.</para>
/// <para>Meaning you can trigger a "sell red potion" task based on natural language, without requiring a speciific formulation.
/// It can of course be used for all sorts of tasks.</para>
/// It requires a <c>NobodyWhoModel</c> node (using the <see cref="NobodyWhoModel"/> wrapper) to be set with a GGUF model capable of generating embeddings.
/// </summary>
public sealed class NobodyWhoEmbedding
{
    private static readonly Variant NullVariant = Variant.From<Node?>(null);

    /// <summary>
    /// Constructs a new instance of the <see cref="NobodyWhoEmbedding"/>.
    /// </summary>
    /// <param name="node">The actual embedding node from the GDExtension.</param>
    /// <exception cref="ArgumentException"></exception>
    /// <exception cref="ArgumentNullException"></exception>
    public NobodyWhoEmbedding(Node node)
    {
        ArgumentNullException.ThrowIfNull(node);

        if(!node.IsClass(nameof(NobodyWhoEmbedding)))
        {
            throw new ArgumentException($"Node must be of class {nameof(NobodyWhoEmbedding)}", nameof(node));
        }
        
        if(!GodotObject.IsInstanceValid(node) || node.IsQueuedForDeletion())
        {
            throw new ArgumentException($"{nameof(NobodyWhoEmbedding)} node cannot be invalid or queued for deletion.", nameof(node));
        }

        EmbeddingNode = node;

        EmbeddingNode.TreeExiting += () =>
        {
            if(EmbeddingNode.IsQueuedForDeletion() && EmbeddingNode.Owner is null)
            {
                GD.PushWarning($"WARNING: The inner {nameof(EmbeddingNode)} node has been queued for deletion outside the control of this wrapper.");
            }
        };
    }

    #region Properties

    /// <summary>
    /// The actual instance of the GDExtension <c>NobodyWhoEmbedding</c> node.
    /// </summary>
    public Node EmbeddingNode { get; init; }

    /// <summary>
    /// The model wrapper for the <c>NobodyWhoModel</c> <see cref="Node"/> used for the embedding.
    /// Defaults to <strong>null</strong>.
    /// </summary>
    public NobodyWhoModel? Model
    {
        get
        {
            Node? modelNode = (Node?) EmbeddingNode.Get(PropertyName.ModelNode);
            return modelNode is null ?
                null : new(modelNode);
        }
        set
        {
            EmbeddingNode.Set(PropertyName.ModelNode, value is null ?
                NullVariant :
                value.ModelNode);
        }
    }

    #endregion Properties

    #region Methods

    /// <summary>
    /// Calculates the similarity between two embedding vectors.
    /// </summary>
    /// <param name="a">The first of the embedding vectors to compare.</param>
    /// <param name="b">The second of the embedding vectors to compare.</param>
    /// <returns>The <see cref="float"/> that represents the similarity between the two vectors, between 0 and 1, where 1 is the highest similarity</returns>
    public float CosineSimilarity(IEnumerable<float> a, IEnumerable<float> b)
    {
        Variant similarity = EmbeddingNode.Call(MethodName.CosineSimilarity, a.ToArray(), b.ToArray());

        return similarity.As<float>();
    }

    /// <summary>
    /// Generates the embedding of the provided <paramref name="text"/> string.
    /// </summary>
    /// <param name="text">The text to generate the embedding for.</param>
    /// <param name="timeout">The optional amount of time to wait before timing out. Defaults to <strong>10 seconds</strong>.</param>
    /// <param name="cancellationToken">The optional cancellation token which can be used to cancel the action.</param>
    /// <returns>The <see cref="Task"/> that represents the asynchronous operation, containing the
    /// collection of <see cref="float"/> embedding values.</returns>
    public async Task<List<float>> EmbedAsync(string text, TimeSpan? timeout = null,
        CancellationToken cancellationToken = default)
    {
        timeout ??= TimeSpan.FromSeconds(10);

        Variant[]? result = await EmbeddingNode.AwaitCallAsync(MethodName.Embed, timeout.Value,
            cancellationToken, text);

        if(result is not null && result.Length > 0 && result[0].Obj is float[] embeddings)
        {
            return [.. embeddings];
        }

        return [];
    }

    /// <summary>
    /// Sets the (global) log level of NobodyWho.
    /// </summary>
    /// <param name="level">The log level to set.</param>
    public void SetLogLevel(LogLevel level)
    {
        EmbeddingNode.Call(MethodName.SetLogLevel, level.ToString().ToUpperInvariant());
    }

    /// <summary>
    /// Starts the embedding worker thread. This is called automatically when you call <see cref="EmbedAsync(string)"/> if it wasn't already called.
    /// This function is blocking and can be a bit slow, so you may want to be strategic about when you call it.
    /// </summary>
    public void StartWorker()
    {
        EmbeddingNode.Call(MethodName.StartWorker);
    }

    /// <summary>
    /// Starts the embedding worker thread. This is required before you can start doing embeddings.
    /// This function is async so it can be called without blocking the main thread. However, the embedding worker won't be started
    /// until the task itself is finished.
    /// </summary>
    /// <param name="cancellationToken">The cancellation token.</param>
    /// <returns>The <see cref="Task"/> that represents the asynchronous operation to start the embedding worker.</returns>
    public Task StartWorkerAsync(CancellationToken cancellationToken = default)
    {
        return Task.Run(() =>
        {
            EmbeddingNode.CallThreadSafe(MethodName.StartWorker);
        }, cancellationToken);
    }

    #endregion Methods

    #region Factory

    /// <summary>
    /// Creates a new instance of the <see cref="NobodyWhoEmbedding"/> wrapper and adds the inner GDExtension embedding node as a child to the provided <paramref name="parent"/> node.
    /// </summary>
    /// <param name="parent">The node to parent the newly created inner GDExtension embedding node to.</param>
    /// <param name="name">The optional name used to set the name of the inner GDExtension embedding node. Defaults to <strong>null</strong>.</param>
    /// <returns>The <see cref="NobodyWhoEmbedding"/> that represents the wrapper of the newly created inner GDExtension embedding node.</returns>
    /// <exception cref="ArgumentException"></exception>
    /// <exception cref="ArgumentNullException"></exception>
    /// <exception cref="InvalidOperationException"></exception>
    public static NobodyWhoEmbedding Create(Node parent, string? name = null)
    {
        ArgumentNullException.ThrowIfNull(parent);

        if(!GodotObject.IsInstanceValid(parent) || parent.IsQueuedForDeletion())
        {
            throw new ArgumentException("Node parent cannot be invalid or queued for deletion.", nameof(parent));
        }

        Node? embeddingNode = (Node?) ClassDB.Instantiate(nameof(NobodyWhoEmbedding))
            ?? throw new InvalidOperationException($"Failed to instantiate GDExtension class {nameof(NobodyWhoEmbedding)}.");

        if(!string.IsNullOrWhiteSpace(name))
        {
            embeddingNode.Name = name;
        }

        parent.AddChild(embeddingNode);

        return new(embeddingNode);
    }

    #endregion Factory

    #region Names

    /// <summary>
    /// Cached StringNames for the properties and fields contained in this class, for fast lookup.
    /// </summary>
    private static class PropertyName
    {
        /// <summary>
        /// <strong>model_node</strong>
        /// </summary>
        public static readonly StringName ModelNode = "model_node";
    }

    /// <summary>
    /// Cached StringNames for the methods contained in this class, for fast lookup.
    /// </summary>
    private static class MethodName
    {
        /// <summary>
        /// <strong>cosine_similarity</strong>
        /// </summary>
        public static readonly StringName CosineSimilarity = "cosine_similarity";

        /// <summary>
        /// <strong>embed</strong>
        /// </summary>
        public static readonly StringName Embed = "embed";

        /// <summary>
        /// <strong>set_log_level</strong>
        /// </summary>
        public static readonly StringName SetLogLevel = "set_log_level";

        /// <summary>
        /// <strong>start_worker</strong>
        /// </summary>
        public static readonly StringName StartWorker = "start_worker";
    }

    /// <summary>
    /// Cached StringNames for the signals contained in this class, for fast lookup.
    /// </summary>
    private static class SignalName
    {
        /// <summary>
        /// <strong>embedding_finished</strong>
        /// </summary>
        public static readonly StringName EmbeddingFinished = "embedding_finished";
    }

    #endregion Names
}