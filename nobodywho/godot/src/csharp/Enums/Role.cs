namespace NobodyWho.Enums;

/// <summary>
/// The enum that represents the various roles that can be assigned to a chat message.
/// </summary>
public enum Role
{
    /// <summary>
    /// The <strong>Assistant</strong> role, associated with messages sent from the LLM.
    /// </summary>
    Assistant,

    /// <summary>
    /// The <strong>System</strong> role, associated with messages set by the system (e.g., a system prompt).
    /// </summary>
    System,


    /// <summary>
    /// The <strong>Tool</strong> role, associated with messages created by a tool.
    /// </summary>
    Tool,

    /// <summary>
    /// The <strong>User</strong> role, associated with messages sent by the user.
    /// </summary>
    User
}