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
/// <para>The wrapper class for the GDExtension <c>NobodyWhoCrossEncoder</c> <see cref="Node"/>, which shares the same name.</para>
/// <para><strong>The <c>NobodyWhoCrossEncoder</c> node is used to rank documents based on their relevance to a query. This is useful for document retrieval and information retrieval tasks.</strong></para>
/// <para>It requires a "NobodyWhoModel" node to be set with a GGUF model capable of reranking.
/// It requires a call to <see cref="StartWorker"/> or <see cref="StartWorkerAsync(CancellationToken)"/> before it can be used.
/// If you do not call it, the cross encoder will start the worker when you send the first message.</para>
/// </summary>
public class NobodyWhoCrossEncoder
{
    private static readonly Variant NullVariant = Variant.From<Node?>(null);

    /// <summary>
    /// Constructs a new instance of the <see cref="NobodyWhoCrossEncoder"/>.
    /// </summary>
    /// <param name="node">The actual cross encoder node from the GDExtension.</param>
    /// <exception cref="ArgumentException"></exception>
    /// <exception cref="ArgumentNullException"></exception>
    public NobodyWhoCrossEncoder(Node node)
    {
        ArgumentNullException.ThrowIfNull(node);

        if(!node.IsClass(nameof(NobodyWhoCrossEncoder)))
        {
            throw new ArgumentException($"Node must be of class {nameof(NobodyWhoCrossEncoder)}", nameof(node));
        }

        if(!GodotObject.IsInstanceValid(node) || node.IsQueuedForDeletion())
        {
            throw new ArgumentException($"{nameof(NobodyWhoCrossEncoder)} node cannot be invalid or queued for deletion.", nameof(node));
        }

        CrossEncoderNode = node;

        CrossEncoderNode.TreeExiting += () =>
        {
            if(CrossEncoderNode.IsQueuedForDeletion() && CrossEncoderNode.Owner is null)
            {
                GD.PushWarning($"WARNING: The inner {nameof(CrossEncoderNode)} node has been queued for deletion outside the control of this wrapper.");
            }
        };
    }

    #region Properties

    /// <summary>
    /// The actual instance of the GDExtension <c>NobodyWhoCrossEncoder</c> node.
    /// </summary>
    public Node CrossEncoderNode { get; init; }

    /// <summary>
    /// The model wrapper for the <c>NobodyWhoModel</c> <see cref="Node"/> used for the cross encoder.
    /// Defaults to <strong>null</strong>.
    /// </summary>
    public NobodyWhoModel? Model
    {
        get
        {
            Node? modelNode = (Node?) CrossEncoderNode.Get(PropertyName.ModelNode);
            return modelNode is null ?
                null : new(modelNode);
        }
        set
        {
            CrossEncoderNode.Set(PropertyName.ModelNode, value is null ?
                NullVariant :
                value.ModelNode);
        }
    }

    #endregion Properties

    #region Methods

    /// <summary>
    /// Ranks documents based on their relevance to the query.
    /// </summary>
    /// <param name="query">The question or query to rank documents against.</param>
    /// <param name="documents">The collection of documents (strings) to rank.</param>
    /// <param name="limit">The maximum number of documents to return. <strong>-1</strong> for all documents. Defaults to <strong>-1</strong>.</param>
    /// <param name="timeout">The optional amount of time to wait before timing out. Defaults to <strong>10 seconds</strong>.</param>
    /// <param name="cancellationToken">The optional cancellation token which can be used to cancel the action.</param>
    /// <returns>The <see cref="Task"/> that represents the asynchronous operation, containing the
    /// collection of ranked documents.</returns>
    /// <exception cref="ArgumentException"></exception>
    /// <exception cref="ArgumentNullException"></exception>
    public async Task<List<string>> RankAsync(string query, IEnumerable<string> documents, int limit = -1, TimeSpan? timeout = null,
        CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(query);
        ArgumentNullException.ThrowIfNull(documents);
        if(!documents.Any())
        {
            throw new ArgumentException("Collection cannot be empty.", nameof(documents));
        }
        if(limit < -1)
        {
            throw new ArgumentException("Cannot be below -1.", nameof(limit));
        }

        timeout ??= TimeSpan.FromSeconds(10);

        Variant[]? result = await CrossEncoderNode.AwaitCallAsync(MethodName.Rank, timeout.Value,
            cancellationToken, query, documents.ToArray(), limit);

        if(result is not null && result.Length > 0 && result[0].Obj is string[] rankings)
        {
            return [.. rankings];
        }

        return [];
    }

    /// <summary>
    /// Ranks documents based on their relevance to the query.
    /// </summary>
    /// <param name="query">The question or query to rank documents against.</param>
    /// <param name="documents">The collection of documents (strings) to rank.</param>
    /// <param name="limit">The maximum number of documents to return. <strong>-1</strong> for all documents. Defaults to <strong>-1</strong>.</param>
    /// <returns>The <see cref="List{T}"/> (of <see cref="string"/>) containing the collection of ranked documents.</returns>
    /// <exception cref="ArgumentException"></exception>
    /// <exception cref="ArgumentNullException"></exception>
    public List<string> Rank(string query, IEnumerable<string> documents, int limit = -1)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(query);
        ArgumentNullException.ThrowIfNull(documents);
        if(!documents.Any())
        {
            throw new ArgumentException("Collection cannot be empty.", nameof(documents));
        }
        if(limit < -1)
        {
            throw new ArgumentException("Cannot be below -1.", nameof(limit));
        }

        Variant? result = CrossEncoderNode.Call(MethodName.RankSync, query, documents.ToArray(), limit);

        if(result is not null && result.Value.Obj is string[] rankings)
        {
            return [.. rankings];
        }

        return [];
    }
    
    /// <summary>
    /// Sets the (global) log level of NobodyWho.
    /// </summary>
    /// <param name="level">The log level to set.</param>
    public void SetLogLevel(LogLevel level)
    {
        CrossEncoderNode.Call(MethodName.SetLogLevel, level.ToString().ToUpperInvariant());
    }

    /// <summary>
    /// Starts the cross encoder worker thread. This is called automatically when you call <see cref="RankAsync"/> or <see cref="Rank"/>, if it wasn't already called.
    /// This function is blocking and can be a bit slow, so you may want to be strategic about when you call it.
    /// </summary>
    public void StartWorker()
    {
        CrossEncoderNode.Call(MethodName.StartWorker);
    }

    /// <summary>
    /// Starts the cross encoder worker thread. This is called automatically when you call <see cref="RankAsync"/> or <see cref="Rank"/>, if it wasn't already called.
    /// This function is async so it can be called without blocking the main thread. However, the LLM won't be started
    /// until the task itself is finished.
    /// </summary>
    /// <param name="cancellationToken">The cancellation token.</param>
    /// <returns>The <see cref="Task"/> that represents the asynchronous operation to start the worker.</returns>
    public Task StartWorkerAsync(CancellationToken cancellationToken = default)
    {
        return Task.Run(() =>
        {
            CrossEncoderNode.CallThreadSafe(MethodName.StartWorker);
        }, cancellationToken);
    }

    #endregion Methods

    #region Factory

    /// <summary>
    /// Creates a new instance of the <see cref="NobodyWhoCrossEncoder"/> wrapper and adds the inner GDExtension cross encoder node as a child to the provided <paramref name="parent"/> node.
    /// </summary>
    /// <param name="parent">The node to parent the newly created inner GDExtension cross encoder node to.</param>
    /// <param name="name">The optional name used to set the name of the inner GDExtension cross encoder node. Defaults to <strong>null</strong>.</param>
    /// <returns>The <see cref="NobodyWhoCrossEncoder"/> that represents the wrapper of the newly created inner GDExtension cross encoder node.</returns>
    /// <exception cref="ArgumentException"></exception>
    /// <exception cref="ArgumentNullException"></exception>
    /// <exception cref="InvalidOperationException"></exception>
    public static NobodyWhoCrossEncoder Create(Node parent, string? name = null)
    {
        ArgumentNullException.ThrowIfNull(parent);

        if(!GodotObject.IsInstanceValid(parent) || parent.IsQueuedForDeletion())
        {
            throw new ArgumentException("Node parent cannot be invalid or queued for deletion.", nameof(parent));
        }

        Node? crossEncoderNode = (Node?) ClassDB.Instantiate(nameof(NobodyWhoCrossEncoder))
            ?? throw new InvalidOperationException($"Failed to instantiate GDExtension class {nameof(NobodyWhoCrossEncoder)}.");

        if(!string.IsNullOrWhiteSpace(name))
        {
            crossEncoderNode.Name = name;
        }

        parent.AddChild(crossEncoderNode);

        return new(crossEncoderNode);
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
        /// <strong>rank</strong>
        /// </summary>
        public static readonly StringName Rank = "rank";

        /// <summary>
        /// <strong>rank_sync</strong>
        /// </summary>
        public static readonly StringName RankSync = "rank_sync";

        /// <summary>
        /// <strong>set_log_level</strong>
        /// </summary>
        public static readonly StringName SetLogLevel = "set_log_level";

        /// <summary>
        /// <strong>start_worker</strong>
        /// </summary>
        public static readonly StringName StartWorker = "start_worker";
    }

    #endregion Names
}