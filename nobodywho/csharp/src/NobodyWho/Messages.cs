using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace NobodyWho;

/// <summary>
/// One piece of a message or prompt: a run of text, or a media file at this position.
/// </summary>
/// <example>
/// <code>
/// var prompt = new Prompt(
///     new ContentPart.Text("What do you see?"),
///     new ContentPart.Image("./photo.png"));
/// </code>
/// </example>
[JsonPolymorphic(TypeDiscriminatorPropertyName = "type")]
[JsonDerivedType(typeof(Text), "text")]
[JsonDerivedType(typeof(Image), "image")]
[JsonDerivedType(typeof(Audio), "audio")]
public abstract record ContentPart
{
    private ContentPart() { }

    /// <summary>A run of text.</summary>
    public sealed record Text(string Value) : ContentPart;

    /// <summary>An image, given as a local file path.</summary>
    public sealed record Image(string Path) : ContentPart;

    /// <summary>An audio clip, given as a local file path.</summary>
    public sealed record Audio(string Path) : ContentPart;

    internal Native.ContentPart ToNative() => this switch
    {
        Text t => new Native.ContentPart.Text(t.Value),
        Image i => new Native.ContentPart.Image(i.Path),
        Audio a => new Native.ContentPart.Audio(a.Path),
        _ => throw new System.InvalidOperationException($"Unknown content part {GetType()}"),
    };

    internal static ContentPart FromNative(Native.ContentPart part) => part switch
    {
        Native.ContentPart.Text t => new Text(t.TextValue),
        Native.ContentPart.Image i => new Image(i.Path),
        Native.ContentPart.Audio a => new Audio(a.Path),
        _ => throw new System.InvalidOperationException($"Unknown content part {part.GetType()}"),
    };
}

/// <summary>
/// The content of a chat message: plain text, interleaved text and media, or JSON.
/// </summary>
/// <remarks>
/// A <see cref="string"/> converts implicitly to <see cref="MessageContent.Text"/>, so
/// <c>new Message.User("Hi")</c> works.
/// </remarks>
[JsonPolymorphic(TypeDiscriminatorPropertyName = "type")]
[JsonDerivedType(typeof(Text), "text")]
[JsonDerivedType(typeof(Parts), "parts")]
[JsonDerivedType(typeof(Json), "json")]
public abstract record MessageContent
{
    private MessageContent() { }

    /// <summary>Content holding a single run of text.</summary>
    public sealed record Text(string Value) : MessageContent;

    /// <summary>Content holding interleaved text and media.</summary>
    public sealed record Parts : MessageContent
    {
        /// <summary>Create content from the given parts, in order.</summary>
        [JsonConstructor]
        public Parts(IReadOnlyList<ContentPart> items) => Items = items;

        /// <summary>Create content from the given parts, in order.</summary>
        public Parts(params ContentPart[] items) : this((IReadOnlyList<ContentPart>)items) { }

        /// <summary>The parts, in order.</summary>
        public IReadOnlyList<ContentPart> Items { get; }

        /// <summary>Deconstruct into the parts.</summary>
        public void Deconstruct(out IReadOnlyList<ContentPart> items) => items = Items;
    }

    /// <summary>
    /// JSON-encoded content. Chat templates written for structured content receive it as a
    /// real list or map rather than as a string.
    /// </summary>
    public sealed record Json(string Value) : MessageContent;

    /// <summary>Wrap a string as <see cref="Text"/> content.</summary>
    public static implicit operator MessageContent(string text) => new Text(text);

    /// <summary>
    /// This content as text. Media parts are left out, so for content that interleaves text and
    /// media this is only the text around it; match on the content itself when the media matters.
    /// </summary>
    public string PlainText => this switch
    {
        Text t => t.Value,
        Json j => j.Value,
        Parts p => string.Concat(p.Items.OfType<ContentPart.Text>().Select(t => t.Value)),
        _ => string.Empty,
    };

    internal Native.MessageContent ToNative() => this switch
    {
        Text t => new Native.MessageContent.Text(t.Value),
        Parts p => new Native.MessageContent.Parts(p.Items.Select(i => i.ToNative()).ToArray()),
        Json j => new Native.MessageContent.Json(j.Value),
        _ => throw new System.InvalidOperationException($"Unknown content {GetType()}"),
    };

    internal static MessageContent FromNative(Native.MessageContent content) => content switch
    {
        Native.MessageContent.Text t => new Text(t.TextValue),
        Native.MessageContent.Parts p => new Parts(p.PartsValue.Select(ContentPart.FromNative).ToArray()),
        Native.MessageContent.Json j => new Json(j.JsonValue),
        _ => throw new System.InvalidOperationException($"Unknown content {content.GetType()}"),
    };
}

/// <summary>A tool invocation requested by the model.</summary>
/// <param name="Name">Name of the tool that was called.</param>
/// <param name="ArgumentsJson">The call's arguments, as a JSON object.</param>
public sealed record ToolCall(string Name, string ArgumentsJson)
{
    /// <summary>The call's arguments, parsed.</summary>
    public JsonElement Arguments
    {
        get
        {
            using var doc = JsonDocument.Parse(ArgumentsJson);
            return doc.RootElement.Clone();
        }
    }
}

/// <summary>
/// A message in the chat history.
/// </summary>
/// <remarks>
/// <list type="bullet">
/// <item><see cref="User"/>: a user message, whose content may interleave text and media.</item>
/// <item><see cref="Assistant"/>: an assistant response, optionally with tool calls.</item>
/// <item><see cref="System"/>: a system prompt.</item>
/// <item><see cref="Tool"/>: the result returned by a tool invocation.</item>
/// </list>
/// <para>
/// Messages serialize with <see cref="JsonSerializer"/>, so a conversation from
/// <see cref="Chat.GetChatHistoryAsync"/> can be saved and passed back to <see cref="Chat.Complete"/> or
/// <see cref="Chat.SetChatHistoryAsync"/> later.
/// </para>
/// </remarks>
[JsonPolymorphic(TypeDiscriminatorPropertyName = "role")]
[JsonDerivedType(typeof(User), "user")]
[JsonDerivedType(typeof(Assistant), "assistant")]
[JsonDerivedType(typeof(System), "system")]
[JsonDerivedType(typeof(Tool), "tool")]
public abstract record Message
{
    private Message() { }

    /// <summary>The message's content.</summary>
    public abstract MessageContent Content { get; }

    /// <summary>A user message.</summary>
    public sealed record User(MessageContent Content) : Message
    {
        /// <inheritdoc/>
        public override MessageContent Content { get; } = Content;
    }

    /// <summary>An assistant response, optionally with the tool calls it made.</summary>
    public sealed record Assistant(MessageContent Content, IReadOnlyList<ToolCall>? ToolCalls = null) : Message
    {
        /// <inheritdoc/>
        public override MessageContent Content { get; } = Content;
    }

    /// <summary>A system prompt.</summary>
    public sealed record System(MessageContent Content) : Message
    {
        /// <inheritdoc/>
        public override MessageContent Content { get; } = Content;
    }

    /// <summary>The result returned by the tool called <paramref name="Name"/>.</summary>
    public sealed record Tool(string Name, MessageContent Content) : Message
    {
        /// <inheritdoc/>
        public override MessageContent Content { get; } = Content;
    }

    internal Native.Message ToNative() => this switch
    {
        User u => new Native.Message.User(u.Content.ToNative()),
        Assistant a => new Native.Message.Assistant(
            a.Content.ToNative(),
            a.ToolCalls?.Select(tc => new Native.ToolCall(tc.Name, tc.ArgumentsJson)).ToArray()),
        System s => new Native.Message.System(s.Content.ToNative()),
        Tool t => new Native.Message.Tool(t.Name, t.Content.ToNative()),
        _ => throw new global::System.InvalidOperationException($"Unknown message {GetType()}"),
    };

    internal static Message FromNative(Native.Message message) => message switch
    {
        Native.Message.User u => new User(MessageContent.FromNative(u.Content)),
        Native.Message.Assistant a => new Assistant(
            MessageContent.FromNative(a.Content),
            a.ToolCalls?.Select(tc => new ToolCall(tc.Name, tc.ArgumentsJson)).ToArray()),
        Native.Message.System s => new System(MessageContent.FromNative(s.Content)),
        Native.Message.Tool t => new Tool(t.Name, MessageContent.FromNative(t.Content)),
        _ => throw new global::System.InvalidOperationException($"Unknown message {message.GetType()}"),
    };
}
