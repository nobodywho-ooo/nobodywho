using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;

namespace NobodyWho;

/// <summary>
/// A prompt made of text, image and audio parts, or of JSON.
/// </summary>
/// <example>
/// <code>
/// var prompt = new Prompt(
///     new ContentPart.Text("What do you see?"),
///     new ContentPart.Image("./photo.png"));
/// var response = await chat.Ask(prompt).CompletedAsync();
///
/// var jsonPrompt = Prompt.FromJson(new { question = "What is 2 + 2?" });
/// </code>
/// </example>
public sealed class Prompt
{
    private readonly ContentPart[] _parts;

    /// <summary>A prompt made of the given parts, in order.</summary>
    public Prompt(params IEnumerable<ContentPart> parts)
    {
        ArgumentNullException.ThrowIfNull(parts);
        _parts = parts.ToArray();
    }

    private Prompt(string json)
    {
        _parts = [];
        Json = json;
    }

    /// <summary>The prompt's parts. Empty for a JSON prompt.</summary>
    public IReadOnlyList<ContentPart> Parts => _parts;

    /// <summary>The prompt as JSON, for a prompt made with <see cref="FromJson"/>.</summary>
    public string? Json { get; }

    /// <summary>
    /// A prompt holding <paramref name="value"/> serialized as JSON. Chat templates written for
    /// structured content receive it as a real list or map rather than as a string.
    /// </summary>
    /// <param name="value">Any value <see cref="JsonSerializer"/> can serialize.</param>
    /// <param name="options">Serializer options. <c>null</c> uses the defaults.</param>
    public static Prompt FromJson<T>(T value, JsonSerializerOptions? options = null) =>
        new(JsonSerializer.Serialize(value, options));

    /// <summary>A prompt holding the given JSON text.</summary>
    public static Prompt FromJsonText(string json)
    {
        ArgumentNullException.ThrowIfNull(json);
        return new(json);
    }

    internal Native.ContentPart[] NativeParts()
    {
        if (Json is not null)
            throw new InvalidOperationException("A JSON prompt has no parts.");
        return _parts.Select(p => p.ToNative()).ToArray();
    }
}
