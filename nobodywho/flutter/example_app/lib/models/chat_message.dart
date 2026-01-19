/// Represents a chat message in the conversation.
class ChatMessage {
  String text;
  final MessageRole role;
  final bool isToolCall;
  final String? toolName;
  final String? toolInput;
  final String? toolResult;

  ChatMessage({
    required this.text,
    required this.role,
    this.isToolCall = false,
    this.toolName,
    this.toolInput,
    this.toolResult,
  });

  /// Creates a user message.
  factory ChatMessage.user(String text) {
    return ChatMessage(text: text, role: MessageRole.user);
  }

  /// Creates an assistant message.
  factory ChatMessage.assistant(String text) {
    return ChatMessage(text: text, role: MessageRole.assistant);
  }

  /// Creates a system message.
  factory ChatMessage.system(String text) {
    return ChatMessage(text: text, role: MessageRole.system);
  }

  /// Creates a tool call message.
  factory ChatMessage.toolCall({
    required String toolName,
    required String toolInput,
    required String toolResult,
  }) {
    return ChatMessage(
      text: '',
      role: MessageRole.tool,
      isToolCall: true,
      toolName: toolName,
      toolInput: toolInput,
      toolResult: toolResult,
    );
  }
}

/// The role of a message sender.
enum MessageRole {
  user,
  assistant,
  system,
  tool,
}
