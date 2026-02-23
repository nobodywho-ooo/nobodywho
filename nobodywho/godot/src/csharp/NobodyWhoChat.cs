using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Godot;
using NobodyWho.Enums;
using NobodyWho.Extensions;
using NobodyWho.Models;

namespace NobodyWho;

/// <summary>
/// <para>The wrapper class for the GDExtension <c>NobodyWhoChat</c> <see cref="Node"/>, which shares the same name.</para>
/// <para><strong><c>NobodyWhoChat</c> is the main node for interacting with the LLM. It functions as a chat, and can be used to send and receive messages.</strong></para>
/// <para>The chat node is used to start a new context to send and receive messages (multiple contexts can be used at the same time with the same model).
/// It requires a call to <see cref="StartWorker"/> or <see cref="StartWorkerAsync(CancellationToken)"/> before it can be used.
/// If you do not call it, the chat will start the worker when you send the first message.</para>
/// </summary>
public sealed class NobodyWhoChat
{
    private static readonly Variant NullVariant = Variant.From<Node?>(null);

    /// <summary>
    /// Constructs a new instance of the <see cref="NobodyWhoChat"/>.
    /// </summary>
    /// <param name="node">The actual chat node from the GDExtension.</param>
    /// <exception cref="ArgumentException"></exception>
    /// <exception cref="ArgumentNullException"></exception>
    public NobodyWhoChat(Node node)
    {
        ArgumentNullException.ThrowIfNull(node);

        if(!node.IsClass(nameof(NobodyWhoChat)))
        {
            throw new ArgumentException($"Node must be of class {nameof(NobodyWhoChat)}", nameof(node));
        }

        if(!GodotObject.IsInstanceValid(node) || node.IsQueuedForDeletion())
        {
            throw new ArgumentException($"{nameof(NobodyWhoChat)} node cannot be invalid or queued for deletion.", nameof(node));
        }

        ChatNode = node;

        ChatNode.TreeExiting += () =>
        {
            if(ChatNode.IsQueuedForDeletion() && ChatNode.Owner is null)
            {
                GD.PushWarning($"WARNING: The inner {nameof(ChatNode)} node has been queued for deletion outside the control of this wrapper.");
            }
        };
    }

    #region Properties

    /// <summary>
    /// The actual instance of the GDExtension <c>NobodyWhoChat</c> node.
    /// </summary>
    public Node ChatNode { get; init; }

    /// <summary>
    /// This is the maximum number of tokens that can be stored in the chat history.
    /// It will delete information from the chat history if it exceeds this limit.
    /// Higher values use more VRAM, but allow for longer "short term memory" for the LLM.
    /// Defaults to <strong>4096</strong>.
    /// </summary>
    public int ContextLength
    {
        get
        {
            return ChatNode.Get(PropertyName.ContextLength).AsInt32();
        }
        set
        {
            ChatNode.Set(PropertyName.ContextLength, value);
        }
    }

    /// <summary>
    /// The model wrapper for the <c>NobodyWhoModel</c> <see cref="Node"/> used for the chat.
    /// Defaults to <strong>null</strong>.
    /// </summary>
    public NobodyWhoModel? Model
    {
        get
        {
            Node? modelNode = (Node?) ChatNode.Get(PropertyName.ModelNode);
            return modelNode is null ? 
                null : new(modelNode);
        }
        set
        {
            ChatNode.Set(PropertyName.ModelNode, value is null ?
                NullVariant :
                value.ModelNode);
        }
    }

    /// <summary>
    /// The sampler wrapper for the <c>NobodyWhoSampler</c> <see cref="Resource"/>used in the configuration of the chat.
    /// Defaults to <strong>null</strong>.
    /// </summary>
    public NobodyWhoSampler? Sampler
    {
        get
        {
            Resource? sampler = (Resource?) ChatNode.Get(PropertyName.Sampler);
            return sampler is null ?
                null : new(sampler);
        }
        set
        {
            ChatNode.Set(PropertyName.Sampler, value is null ?
                NullVariant :
                value.SamplerResource);
        }
    }

    /// <summary>
    /// Stop tokens to stop generation at these specified tokens. Defaults to an <strong>empty list</strong>.
    /// </summary>
    public IReadOnlyList<string> StopWords
    {
        get
        {
            return [.. ChatNode.Get(PropertyName.StopWords).AsGodotArray<string>()];
        }
        set
        {
            ChatNode.Set(PropertyName.StopWords, value.ToArray());
        }
    }

    /// <summary>
    /// The system prompt for the chat, this is the basic instructions for the LLM's behavior.
    /// Defaults to a <see cref="string.Empty"/>.
    /// </summary>
    public string SystemPrompt
    {
        get
        {
            return ChatNode.Get(PropertyName.SystemPrompt).AsString();
        }
        set
        {
            ChatNode.Set(PropertyName.SystemPrompt, value);
        }
    }

    #endregion Properties

    #region Actions

    /// <summary>
    /// Triggered when a new token is received from the LLM. Returns the new token as a string.
    /// It is strongly recommended to connect to this signal, and display the text output as it is being generated.
    /// This makes for a much nicer user experience.
    /// </summary>
    public event Action<string> ResponseUpdated
    {
        add => ChatNode.Connect(SignalName.ResponseUpdated, Callable.From(value));
        remove => ChatNode.Disconnect(SignalName.ResponseUpdated, Callable.From(value));
    }

    /// <summary>
    /// Triggered when the LLM has finished generating the response. Returns the full response as a string.
    /// </summary>
    public event Action<string> ResponseFinished
    {
        add => ChatNode.Connect(SignalName.ResponseFinished, Callable.From(value));
        remove => ChatNode.Disconnect(SignalName.ResponseFinished, Callable.From(value));
    }

    #endregion Actions

    #region Methods

    /// <summary>
    /// <para>Add a tool for the LLM to use. Tool calling is only supported for a select few models. We recommend Qwen3.</para>
    /// <para>The tool is a fully typed callable function on a godot object. The function should return a string.
    /// All parameters should have type hints, and only primitive types are supported.
    /// NobodyWho will use the type hints to constrain the generation, such that the function will only ever be called with the correct types.
    /// Fancier types like lists, dictionaries, and classes are not (yet) supported.</para>
    /// <para>If you need to specify more parameter constraints, see <see cref="AddToolWithSchema(GodotObject, string, string, string)"/>.</para>
    /// <para>Example:</para>
    /// <code>
    /// public string AddNumbers(int a, int b)
    /// {
    ///     return (a + b).ToString();
    /// }
    /// 
    /// public void override _Ready()
    /// {
    ///     NobodyWhoChat chatWrapper = NobodyWhoChat.New(this);
    ///     
    ///     // Register the tool.
    ///     chatWrapper.AddTool(this, nameof(AddNumbers), "Adds two integers");
    ///     
    ///     // See that the LLM invokes the tool.
    ///     chatWrapper.Say("What is two plus two?");
    /// }
    /// </code>
    /// </summary>
    /// <param name="target">The object that will call the method matching the provided <paramref name="methodName"/>.</param>
    /// <param name="methodName">The name of the method to invoke on the tool call.</param>
    /// <param name="description">The description text used to match against to trigger the tool call.</param>
    /// <exception cref="ArgumentNullException"></exception>
    public void AddTool(GodotObject target, string methodName, string description)
    {
        ArgumentNullException.ThrowIfNull(target);
        ArgumentException.ThrowIfNullOrWhiteSpace(methodName);

        AddTool(new Callable(target, methodName), description);
    }

    /// <summary>
    /// <para>Add a tool for the LLM to use. Tool calling is only supported for a select few models. We recommend Qwen3.</para>
    /// <para>The tool is a fully typed callable function on a godot object. The function should return a string.
    /// All parameters should have type hints, and only primitive types are supported.
    /// NobodyWho will use the type hints to constrain the generation, such that the function will only ever be called with the correct types.
    /// Fancier types like lists, dictionaries, and classes are not (yet) supported.</para>
    /// <para>If you need to specify more parameter constraints, see <see cref="AddToolWithSchema(Callable, string, string)"/>.</para>
    /// <para>Example:</para>
    /// <code>
    /// public string AddNumbers(int a, int b)
    /// {
    ///     return (a + b).ToString();
    /// }
    /// 
    /// public void override _Ready()
    /// {
    ///     NobodyWhoChat chatWrapper = NobodyWhoChat.New(this);
    ///     
    ///     // Register the tool.
    ///     chatWrapper.AddTool(this, nameof(AddNumbers), "Adds two integers");
    ///     
    ///     // See that the LLM invokes the tool.
    ///     chatWrapper.Say("What is two plus two?");
    /// }
    /// </code>
    /// </summary>
    /// <param name="callable">The callable to trigger with the tool call.</param>
    /// <param name="description">The description text used to match against to trigger the tool call.</param>
    /// <exception cref="ArgumentNullException"></exception>
    public void AddTool(Callable callable, string description)
    {
        ArgumentNullException.ThrowIfNull(callable);
        ArgumentException.ThrowIfNullOrWhiteSpace(description);

        ChatNode.Call(MethodName.AddTool, callable, description);
    }

    /// <summary>
    /// <para>Add a tool for the LLM to use, along with a json schema to constrain the parameters.
    /// The order of parameters in the json schema must be preserved.
    /// The json schema keyword "description" may be used here, to help guide the LLM.
    /// Tool calling is only supported for a select few models. We recommend Qwen3.</para>
    /// <para>Example:</para>
    /// <code>
    /// public string AddNumbers(int a, int b)
    /// {
    ///     return (a + b).ToString();
    /// }
    /// 
    /// public void override _Ready()
    /// {
    ///     string jsonSchema = @"
    ///     {
    ///         ""type"": ""object"",
    ///         ""properties"": {
    ///             ""a"": { ""type"": "integer" },
    ///             ""b"": { ""type"": "integer" }
    ///         },
    ///         ""required"": [""a"", ""b""],
    ///     }";
    ///     
    ///     // Register the tool.
    ///     AddToolWithSchema(this, nameof(AddNumbers), "Adds two integers", jsonSchema);
    ///     
    ///     // See that the LLM invokes the tool.
    ///     Say("What is two plus two?");
    /// }
    /// </code>
    /// </summary>
    /// <param name="target">The object that will call the method matching the provided <paramref name="methodName"/>.</param>
    /// <param name="methodName">The name of the method to invoke on the tool call.</param>
    /// <param name="description">The description text used to match against to trigger the tool call.</param>
    /// <param name="jsonSchema">The schema used to constrain the parameters.</param>
    /// <exception cref="ArgumentException"></exception>
    /// <exception cref="ArgumentNullException"></exception>
    /// <exception cref="JsonException"></exception>
    public void AddToolWithSchema(GodotObject target, string methodName, string description, string jsonSchema)
    {
        ArgumentNullException.ThrowIfNull(target);
        ArgumentException.ThrowIfNullOrWhiteSpace(methodName);

        AddToolWithSchema(new Callable(target, methodName), description, jsonSchema);
    }

    /// <summary>
    /// <para>Add a tool for the LLM to use, along with a json schema to constrain the parameters.
    /// The order of parameters in the json schema must be preserved.
    /// The json schema keyword "description" may be used here, to help guide the LLM.
    /// Tool calling is only supported for a select few models. We recommend Qwen3.</para>
    /// <para>Example:</para>
    /// <code>
    /// public string AddNumbers(int a, int b)
    /// {
    ///     return (a + b).ToString();
    /// }
    /// 
    /// public void override _Ready()
    /// {
    ///     string jsonSchema = @"
    ///     {
    ///         ""type"": ""object"",
    ///         ""properties"": {
    ///             ""a"": { ""type"": "integer" },
    ///             ""b"": { ""type"": "integer" }
    ///         },
    ///         ""required"": [""a"", ""b""],
    ///     }";
    ///     
    ///     // Register the tool.
    ///     AddToolWithSchema(this, nameof(AddNumbers), "Adds two integers", jsonSchema);
    ///     
    ///     // See that the LLM invokes the tool.
    ///     Say("What is two plus two?");
    /// }
    /// </code>
    /// </summary>
    /// <param name="callable">The callable to trigger with the tool call.</param>
    /// <param name="description">The description text used to match against to trigger the tool call.</param>
    /// <param name="jsonSchema">The schema used to constrain the parameters.</param>
    /// <exception cref="ArgumentException"></exception>
    /// <exception cref="ArgumentNullException"></exception>
    /// <exception cref="JsonException"></exception>
    public void AddToolWithSchema(Callable callable, string description, string jsonSchema)
    {
        ArgumentNullException.ThrowIfNull(callable);
        ArgumentException.ThrowIfNullOrWhiteSpace(description);
        ArgumentException.ThrowIfNullOrWhiteSpace(jsonSchema);

        // Will throw a JsonException if the provided json schema is NOT valid.
        using JsonDocument _ = JsonDocument.Parse(jsonSchema);

        ChatNode.Call(MethodName.AddToolWithSchema, callable, description, jsonSchema);
    }

    /// <summary>
    /// Removes a tool from the LLM. Tool calling is only supported for a select few models. We recommend Qwen3.
    /// </summary>
    /// <param name="target">The object that was going to call the method matching the provided <paramref name="methodName"/>.</param>
    /// <param name="methodName">The name of the method to remove that was being invoked on the tool call.</param>
    /// <exception cref="ArgumentNullException"></exception>
    public void RemoveTool(GodotObject target, string methodName)
    {
        ArgumentNullException.ThrowIfNull(target);
        ArgumentException.ThrowIfNullOrWhiteSpace(methodName);

        RemoveTool(new Callable(target, methodName));
    }

    /// <summary>
    /// Removes a tool from the LLM. Tool calling is only supported for a select few models. We recommend Qwen3.
    /// </summary>
    /// <param name="callable">The callable to remove as a tool call.</param>
    /// <exception cref="ArgumentNullException"></exception>
    public void RemoveTool(Callable callable)
    {
        ArgumentNullException.ThrowIfNull(callable);

        ChatNode.Call(MethodName.RemoveTool, callable);
    }

    /// <summary>
    /// Gets the chat messages stored in the context.
    /// </summary>
    /// <param name="timeout">The optional amount of time to wait before timing out. Defaults to <strong>10 seconds</strong>.</param>
    /// <param name="cancellationToken">The optional cancellation token which can be used to cancel the action.</param>
    /// <returns>The <see cref="Task"/> that represents the asynchronous operation, containing the
    /// collection of <see cref="ChatMessage"/> objects.</returns>
    public async Task<List<ChatMessage>> GetChatHistoryAsync(TimeSpan? timeout = null,
        CancellationToken cancellationToken = default)
    {
        timeout ??= TimeSpan.FromSeconds(10);

        Variant[]? result = await ChatNode.AwaitCallAsync(MethodName.GetChatHistory, timeout.Value,
            cancellationToken);

        List<ChatMessage> chatHistory = [];

        if(result is not null && result.Length > 0 && result[0].Obj is Godot.Collections.Array messages)
        {
            foreach(Variant message in messages)
            {
                if(message.Obj is Godot.Collections.Dictionary chatMessage)
                {
                    chatHistory.Add(new(chatMessage));
                }
            }
        }

        return chatHistory;
    }

    /// <summary>
    /// Clears all history and context for the worker.
    /// It will still retain all prior settings (such as system prompt).
    /// </summary>
    public void ResetContext()
    {
        ChatNode.Call(MethodName.ResetContext);
    }

    /// <summary>
    /// Sends a message to the LLM. This will start the inference process, meaning you can also listen on the <see cref="ResponseUpdated"/> and <see cref="ResponseFinished"/> signals to get the response.
    /// </summary>
    /// <param name="message">The message to send to the LLM.</param>
    public void Say(string message)
    {
        ChatNode.Call(MethodName.Say, message);
    }

    /// <summary>
    /// Gets the response text by awaiting on the <strong>response_finished</strong> signal.
    /// </summary>
    /// <remarks>
    /// <strong>NOTE:</strong> This is mostly used for testing and convenience, and is recommeneded to instead setup the <see cref="ResponseUpdated"/> and/or <see cref="ResponseFinished"/> signals.
    /// This will work even if there are actions registered on the <see cref="ResponseFinished"/> action, where this and the actions will both run.
    /// </remarks>
    /// <param name="timeout">The optional amount of time to wait before timing out. Defaults to <strong>10 seconds</strong>.</param>
    /// <param name="cancellationToken">The optional cancellation token which can be used to cancel the action.</param>
    /// <returns>The <see cref="Task"/> that represents the asynchronous operation,
    /// containing response text <see cref="string"/>.</returns>
    public async Task<string> GetResponseAsync(TimeSpan? timeout = null, CancellationToken cancellationToken = default)
    {
        timeout ??= TimeSpan.FromSeconds(10);

        Variant[]? result = await ChatNode.AwaitSignalAsync(ChatNode, SignalName.ResponseFinished, timeout.Value,
            cancellationToken);

        if(result is not null && result.Length > 0 && result[0].Obj is string responseText)
        {
            return responseText;
        }

        return string.Empty;
    }

    /// <summary>
    /// Sets the context to that of the provided <paramref name="messages"/> (useful for templates or saved states).
    /// </summary>
    /// <param name="messages">The collection of chat messages to set into the context.</param>
    /// <exception cref="ArgumentNullException"></exception>
    public void SetChatHistory(IEnumerable<ChatMessage> messages)
    {
        ArgumentNullException.ThrowIfNull(messages);

        Godot.Collections.Array chatHistory = [];

        foreach(ChatMessage message in messages)
        {
            chatHistory.Add(message.ToGodotDictionary());
        }

        ChatNode.Call(MethodName.SetChatHistory, chatHistory);
    }

    /// <summary>
    /// Sets the (global) log level of NobodyWho.
    /// </summary>
    /// <param name="level">The log level to set.</param>
    public void SetLogLevel(LogLevel level)
    {
        ChatNode.Call(MethodName.SetLogLevel, level.ToString().ToUpperInvariant());
    }

    /// <summary>
    /// Starts the LLM worker thread. This is required before you can send messages to the LLM.
    /// This function is blocking and can be a bit slow, so you may want to be strategic about when you call it.
    /// </summary>
    public void StartWorker()
    {
        ChatNode.Call(MethodName.StartWorker);
    }

    /// <summary>
    /// Starts the LLM worker thread. This is required before you can send messages to the LLM.
    /// This function is async so it can be called without blocking the main thread. However, the LLM won't be started
    /// until the task itself is finished.
    /// </summary>
    /// <param name="cancellationToken">The cancellation token.</param>
    /// <returns>The <see cref="Task"/> that represents the asynchronous operation to start the worker.</returns>
    public Task StartWorkerAsync(CancellationToken cancellationToken = default)
    {
        return Task.Run(() =>
        {
            ChatNode.CallThreadSafe(MethodName.StartWorker);
        }, cancellationToken);
    }

    /// <summary>
    /// Stops the LLM from generating any further tokens if it was in the process of doing so.
    /// </summary>
    /// <remarks>
    /// <strong>NOTE:</strong> Useful if placed within a registered <see cref="ResponseUpdated"/> action to stop generation if a certain token is detected.
    /// </remarks>
    public void StopGeneration()
    {
        ChatNode.Call(MethodName.StopGeneration);
    }

    #endregion Methods

    #region Factory

    /// <summary>
    /// Creates a new instance of the <see cref="NobodyWhoChat"/> wrapper and adds the inner GDExtension chat node as a child to the provided <paramref name="parent"/> node.
    /// </summary>
    /// <param name="parent">The node to parent the newly created inner GDExtension chat node to.</param>
    /// <param name="name">The optional name used to set the name of the inner GDExtension chat node. Defaults to <strong>null</strong>.</param>
    /// <returns>The <see cref="NobodyWhoChat"/> that represents the wrapper of the newly created inner GDExtension chat node.</returns>
    /// <exception cref="ArgumentException"></exception>
    /// <exception cref="ArgumentNullException"></exception>
    /// <exception cref="InvalidOperationException"></exception>
    public static NobodyWhoChat Create(Node parent, string? name = null)
    {
        ArgumentNullException.ThrowIfNull(parent);

        if(!GodotObject.IsInstanceValid(parent) || parent.IsQueuedForDeletion())
        {
            throw new ArgumentException("Node parent cannot be invalid or queued for deletion.", nameof(parent));
        }

        Node? chatNode = (Node?) ClassDB.Instantiate(nameof(NobodyWhoChat))
            ?? throw new InvalidOperationException($"Failed to instantiate GDExtension class {nameof(NobodyWhoChat)}.");

        if(!string.IsNullOrWhiteSpace(name))
        {
            chatNode.Name = name;
        }

        parent.AddChild(chatNode);

        return new(chatNode);
    }

    #endregion Factory

    #region Names

    /// <summary>
    /// Cached StringNames for the properties and fields contained in this class, for fast lookup.
    /// </summary>
    private static class PropertyName
    {
        /// <summary>
        /// <strong>context_length</strong>
        /// </summary>
        public static readonly StringName ContextLength = "context_length";

        /// <summary>
        /// <strong>model_node</strong>
        /// </summary>
        public static readonly StringName ModelNode = "model_node";

        /// <summary>
        /// <strong>sampler</strong>
        /// </summary>
        public static readonly StringName Sampler = "sampler";

        /// <summary>
        /// <strong>stop_words</strong>
        /// </summary>
        public static readonly StringName StopWords = "stop_words";

        /// <summary>
        /// <strong>system_prompt</strong>
        /// </summary>
        public static readonly StringName SystemPrompt = "system_prompt";
    }

    /// <summary>
    /// Cached StringNames for the methods contained in this class, for fast lookup.
    /// </summary>
    private static class MethodName
    {
        /// <summary>
        /// <strong>add_tool</strong>
        /// </summary>
        public static readonly StringName AddTool = "add_tool";

        /// <summary>
        /// <strong>add_tool_with_schema</strong>
        /// </summary>
        public static readonly StringName AddToolWithSchema = "add_tool_with_schema";

        /// <summary>
        /// <strong>get_chat_history</strong>
        /// </summary>
        public static readonly StringName GetChatHistory = "get_chat_history";

        /// <summary>
        /// <strong>remove_tool</strong>
        /// </summary>
        public static readonly StringName RemoveTool = "remove_tool";

        /// <summary>
        /// <strong>reset_context</strong>
        /// </summary>
        public static readonly StringName ResetContext = "reset_context";

        /// <summary>
        /// <strong>say</strong>
        /// </summary>
        public static readonly StringName Say = "say";

        /// <summary>
        /// <strong>set_chat_history</strong>
        /// </summary>
        public static readonly StringName SetChatHistory = "set_chat_history";

        /// <summary>
        /// <strong>set_log_level</strong>
        /// </summary>
        public static readonly StringName SetLogLevel = "set_log_level";

        /// <summary>
        /// <strong>start_worker</strong>
        /// </summary>
        public static readonly StringName StartWorker = "start_worker";

        /// <summary>
        /// <strong>stop_generation</strong>
        /// </summary>
        public static readonly StringName StopGeneration = "stop_generation";
    }

    /// <summary>
    /// Cached StringNames for the signals contained in this class, for fast lookup.
    /// </summary>
    private static class SignalName
    {
        /// <summary>
        /// <strong>response_updated</strong>
        /// </summary>
        public static readonly StringName ResponseUpdated = "response_updated";

        /// <summary>
        /// <strong>response_finished</strong>
        /// </summary>
        public static readonly StringName ResponseFinished = "response_finished";
    }

    #endregion Names
}