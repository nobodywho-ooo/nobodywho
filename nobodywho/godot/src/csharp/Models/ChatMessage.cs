using System;
using System.Diagnostics;
using Godot;
using NobodyWho.Enums;

namespace NobodyWho.Models;

/// <summary>
/// The class that represents a single message in a chat conversation, containing both the message content and associated role.
/// </summary>
[DebuggerDisplay(@"\{Role = '{Role}', Content = {Content}\}")]
public sealed class ChatMessage : IEquatable<ChatMessage>
{
    #region Fields

    private static readonly string RoleKey = nameof(Role).ToLowerInvariant();
    private static readonly string ContentKey = nameof(Content).ToLowerInvariant();

    #endregion Fields

    /// <summary>
    /// Constructs a new instance of the <see cref="ChatMessage"/>.
    /// </summary>
    /// <param name="role">The role to assign to the chat message.</param>
    /// <param name="content">The text content to assign to the chat message.</param>
    /// <exception cref="ArgumentNullException"></exception>
    public ChatMessage(Role role, string content)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(content);

        Role = role;
        Content = content;
    }

    /// <summary>
    /// Constructs a new instance of the <see cref="ChatMessage"/>.
    /// </summary>
    /// <param name="dictionary">The Godot dictionary containing the role and content of a chat message.</param>
    /// <exception cref="ArgumentException"></exception>
    /// <exception cref="ArgumentNullException"></exception>
    public ChatMessage(Godot.Collections.Dictionary dictionary)
    {
        ArgumentNullException.ThrowIfNull(dictionary);
        
        if(dictionary.Count == 0)
        {
            throw new ArgumentException("Godot dictionary cannot be empty.", nameof(dictionary));
        }

        if(dictionary.TryGetValue(RoleKey, out Variant roleValue))
        {
            if(Enum.TryParse(roleValue.AsString(), ignoreCase: true, out Role role))
            {
                Role = role;
            }
            else
            {
                throw new ArgumentException($"Godot dictionary contains an invalid value for the {RoleKey}.", nameof(dictionary));
            }
        }
        else
        {
            throw new ArgumentException($"Godot dictionary is required to have the {RoleKey} key.", nameof(dictionary));
        }

        if(dictionary.TryGetValue(ContentKey, out Variant contentValue))
        {
            Content = contentValue.AsString();
        }
        else
        {
            throw new ArgumentException($"Godot dictionary is required to have the {ContentKey} key.", nameof(dictionary));
        }
    }

    /// <summary>
    /// The role assigned to this chat message (i.e. who sent/set the message).
    /// </summary>
    public Role Role { get; set; }

    /// <summary>
    /// The text content of the message.
    /// </summary>
    public string Content { get; set; }

    /// <summary>
    /// Converts the <see cref="ChatMessage"/> instance into a <see cref="Godot.Collections.Dictionary"/>.
    /// </summary>
    /// <returns>The <see cref="Godot.Collections.Dictionary"/> that represents the converted <see cref="ChatMessage"/>.</returns>
    public Godot.Collections.Dictionary ToGodotDictionary()
    {
        return new Godot.Collections.Dictionary
        {
            [RoleKey] = Role.ToString().ToLowerInvariant(),
            [ContentKey] = Content
        };
    }

    /// <inheritdoc/>
    public override string ToString()
    {
        return $"{{Role = \"{Role}\", Content = \"{Content}\"}}";
    }

    /// <inheritdoc/>
    public override bool Equals(object? obj) => Equals(obj as ChatMessage);

    /// <inheritdoc/>
    public bool Equals(ChatMessage? other)
    {
        if(other is null)
        {
            return false;
        }

        if(ReferenceEquals(this, other))
        {
            return true;
        }

        return Role == other.Role &&
            string.Equals(Content, other.Content, StringComparison.Ordinal);
    }

    /// <inheritdoc/>
    public override int GetHashCode()
    {
        return HashCode.Combine(Role, Content);
    }

    /// <summary>
    /// Overloaded equality operator.
    /// </summary>
    /// <param name="left">The left hand side <see cref="ChatMessage"/> to compare.</param>
    /// <param name="right">The right hand side <see cref="ChatMessage"/> to compare.</param>
    /// <returns>The <see cref="bool"/> that represents whether the provided <see cref="ChatMessage"/> objects are equal or not.</returns>
    public static bool operator ==(ChatMessage? left, ChatMessage? right)
    {
        if(left is null)
        {
            if(right is null)
            {
                return true;
            }

            return false;
        }

        return left.Equals(right);
    }

    /// <summary>
    /// Overloaded inequality operator.
    /// </summary>
    /// <param name="left">The left hand side <see cref="ChatMessage"/> to compare.</param>
    /// <param name="right">The right hand side <see cref="ChatMessage"/> to compare.</param>
    /// <returns>The <see cref="bool"/> that represents whether the provided <see cref="ChatMessage"/> objects are NOT equal or not.</returns>
    public static bool operator !=(ChatMessage? left, ChatMessage? right) => !(left == right);
}