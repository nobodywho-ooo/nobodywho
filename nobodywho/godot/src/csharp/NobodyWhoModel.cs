using System;
using Godot;
using NobodyWho.Enums;
using NobodyWho.Models;

namespace NobodyWho;

/// <summary>
/// <para>The wrapper class for the GDExtension <c>NobodyWhoModel</c> <see cref="Node"/>, which shares the same name.</para>
/// <para><strong>The model node is used to load the model, currently only GGUF models are supported.</strong></para>
/// <para>If you dont know what model to use, we would suggest checking out <a href="https://huggingface.co/spaces/k-mktr/gpu-poor-llm-arena"></a></para>
/// </summary>
public sealed class NobodyWhoModel
{
    /// <summary>
    /// Constructs a new instance of the <see cref="NobodyWhoModel"/>.
    /// </summary>
    /// <param name="node">The actual model node from the GDExtension.</param>
    /// <exception cref="ArgumentException"></exception>
    /// <exception cref="ArgumentNullException"></exception>
    public NobodyWhoModel(Node node)
    {
        ArgumentNullException.ThrowIfNull(node);

        if(!node.IsClass(nameof(NobodyWhoModel)))
        {
            throw new ArgumentException($"Node must be of class {nameof(NobodyWhoModel)}", nameof(node));
        }

        if(!GodotObject.IsInstanceValid(node) || node.IsQueuedForDeletion())
        {
            throw new ArgumentException("NobodyWhoModel node cannot be invalid or queued for deletion.", nameof(node));
        }

        ModelNode = node;

        ModelNode.TreeExiting += () =>
        {
            if(ModelNode.IsQueuedForDeletion() && ModelNode.Owner is null)
            {
                GD.PushWarning($"WARNING: The inner {nameof(ModelNode)} node has been queued for deletion outside the control of this wrapper.");
            }
        };
    }

    #region Properties

    /// <summary>
    /// The actual instance of the GDExtension <c>NobodyWhoModel</c> node.
    /// </summary>
    public Node ModelNode { get; init; }

    /// <summary>
    /// The file path to the <strong>GGUF</strong> (<c>*.gguf</c>) model file.
    /// </summary>
    public string ModelPath
    {
        get
        {
            return ModelNode.Get(PropertyName.ModelPath).AsString();
        }
        set
        {
            ModelNode.Set(PropertyName.ModelPath, value);
        }
    }

    /// <summary>
    /// The flag used to determine if the GPU should be used when running the worker.
    /// </summary>
    public bool UseGpuIfAvailable
    {
        get
        {
            return ModelNode.Get(PropertyName.UseGpuIfAvailable).AsBool();
        }
        set
        {
            ModelNode.Set(PropertyName.UseGpuIfAvailable, value);
        }
    }

    #endregion Properties

    #region Methods

    /// <summary>
    /// Sets the (global) log level of NobodyWho.
    /// </summary>
    /// <param name="level">The log level to set.</param>
    public void SetLogLevel(LogLevel level)
    {
        ModelNode.Call(MethodName.SetLogLevel, level.ToString().ToUpperInvariant());
    }

    #endregion Methods

    #region Factory

    /// <summary>
    /// Creates a new instance of the <see cref="NobodyWhoModel"/> wrapper and adds the inner GDExtension model node as a child to the provided <paramref name="parent"/> node.
    /// </summary>
    /// <param name="parent">The node to parent the newly created inner GDExtension model node to.</param>
    /// <param name="name">The optional name used to set the name of the inner GDExtension model node. Defaults to <strong>null</strong>.</param>
    /// <returns>The <see cref="NobodyWhoModel"/> that represents the wrapper of the newly created inner GDExtension model node.</returns>
    /// <exception cref="ArgumentException"></exception>
    /// <exception cref="ArgumentNullException"></exception>
    /// <exception cref="InvalidOperationException"></exception>
    public static NobodyWhoModel Create(Node parent, string? name = null)
    {
        ArgumentNullException.ThrowIfNull(parent);

        if(!GodotObject.IsInstanceValid(parent) || parent.IsQueuedForDeletion())
        {
            throw new ArgumentException("Node parent cannot be invalid or queued for deletion.", nameof(parent));
        }

        Node? modelNode = (Node?) ClassDB.Instantiate(nameof(NobodyWhoModel))
            ?? throw new InvalidOperationException($"Failed to instantiate GDExtension class {nameof(NobodyWhoModel)}.");

        if(!string.IsNullOrWhiteSpace(name))
        {
            modelNode.Name = name;
        }

        parent.AddChild(modelNode);

        return new(modelNode);
    }

    #endregion Factory

    #region Names

    /// <summary>
    /// Cached StringNames for the properties and fields contained in this class, for fast lookup.
    /// </summary>
    private static class PropertyName
    {
        /// <summary>
        /// <strong>model_path</strong>
        /// </summary>
        public static readonly StringName ModelPath = "model_path";

        /// <summary>
        /// <strong>use_gpu_if_available</strong>
        /// </summary>
        public static readonly StringName UseGpuIfAvailable = "use_gpu_if_available";
    }

    /// <summary>
    /// Cached StringNames for the methods contained in this class, for fast lookup.
    /// </summary>
    private static class MethodName
    {
        /// <summary>
        /// <strong>set_log_level</strong>
        /// </summary>
        public static readonly StringName SetLogLevel = "set_log_level";
    }

    #endregion Names
}