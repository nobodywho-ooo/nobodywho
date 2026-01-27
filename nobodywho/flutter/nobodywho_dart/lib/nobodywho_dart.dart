library;

import 'dart:async';
import 'dart:convert';

export 'src/rust/api/nobodywho.dart'
    hide
        RustChat, // Users should use Chat
        RustTokenStream, // Users should use TokenStream
        RustTool, // Users should use Tool
        newToolImpl, // Internal helper
        toolCallArgumentsJson; // Internal helper
export 'src/rust/frb_generated.dart' show NobodyWho;

import 'src/rust/api/nobodywho.dart' as nobodywho;

/// Extension to provide convenient access to ToolCall arguments.
/// The underlying `arguments` field is an opaque serde_json::Value,
/// so we provide these helper methods to access it as JSON string or Map.
extension ToolCallExtension on nobodywho.ToolCall {
  /// Get the arguments as a JSON string
  String get argumentsJson => nobodywho.toolCallArgumentsJson(toolCall: this);

  /// Get the arguments as a parsed Map
  Map<String, dynamic> get argumentsMap =>
      json.decode(nobodywho.toolCallArgumentsJson(toolCall: this))
          as Map<String, dynamic>;
}

// Wrapper for the RustTool class. We wrap RustTool so the API for constructing a tool
// is simply passing the arguments to a constructor.
class Tool {
  final nobodywho.RustTool _tool;

  /// Private constructor
  Tool._(this._tool);

  /// Create a tool from a Dart function.
  factory Tool({
    required Function function,
    required String name,
    required String description,
  }) {
    // Wrapper needs to be written in Dart to access `function.runtimeType`
    // and to deal with dynamic function parameters

    // Make it a String -> Future<String> function
    final wrappedFunction = (String jsonString) async {
      // Decode the input string as json
      Map<String, dynamic> jsonMap = json.decode(jsonString);
      // Make it a map of symbols, to make Function.apply happy
      Map<Symbol, dynamic> namedParams = Map.fromEntries(
        jsonMap.entries.map((e) => MapEntry(Symbol(e.key), e.value)),
      );

      // Call the function
      final result = Function.apply(function, [], namedParams);

      // Handle async tools and return
      if (result is Future) {
        return (await result).toString();
      } else {
        return result.toString();
      }
    };

    final tool = nobodywho.newToolImpl(
      function: wrappedFunction,
      name: name,
      description: description,
      runtimeType: function.runtimeType.toString(),
    );

    return Tool._(tool);
  }

  /// Internal getter for Chat to access the underlying tool
  nobodywho.RustTool get _internalTool => _tool;
}

/// A stream of response tokens from the model.
/// Implements [Stream<String>] so it can be used with `await for`.
class TokenStream extends Stream<String> {
  final nobodywho.RustTokenStream _tokenStream;

  TokenStream._(this._tokenStream);

  @override
  StreamSubscription<String> listen(
    void Function(String event)? onData, {
    Function? onError,
    void Function()? onDone,
    bool? cancelOnError,
  }) {
    return _generateStream().listen(
      onData,
      onError: onError,
      onDone: onDone,
      cancelOnError: cancelOnError,
    );
  }

  Stream<String> _generateStream() async* {
    while (true) {
      final token = await _tokenStream.nextToken();
      if (token == null) break;
      yield token;
    }
  }

  /// Wait for the complete response and return it as a single string.
  Future<String> completed() => _tokenStream.completed();
}

// Wrapper for the RustChat class. This is necessary to use the functionality
// gained by wrapping RustTool and RustTokenStream.
class Chat {
  final nobodywho.RustChat _chat;

  /// Create chat from existing model.
  Chat({
    required nobodywho.Model model,
    String? systemPrompt,
    int contextSize = 4096,
    bool allowThinking = true,
    List<Tool> tools = const [],
    nobodywho.SamplerConfig? sampler,
  }) : _chat = nobodywho.RustChat(
         model: model,
         systemPrompt: systemPrompt,
         contextSize: contextSize,
         allowThinking: allowThinking,
         tools: tools.map((t) => t._internalTool).toList(),
         sampler: sampler,
       );

  /// Private constructor for wrapping an existing Chat
  Chat._(this._chat);

  /// Create chat directly from a model path.
  static Future<Chat> fromPath({
    required String modelPath,
    String? systemPrompt,
    int contextSize = 4096,
    bool allowThinking = true,
    List<Tool> tools = const [],
    nobodywho.SamplerConfig? sampler,
    bool useGpu = true,
  }) async {
    final chat = await nobodywho.RustChat.fromPath(
      modelPath: modelPath,
      systemPrompt: systemPrompt,
      contextSize: contextSize,
      allowThinking: allowThinking,
      tools: tools.map((t) => t._internalTool).toList(),
      sampler: sampler,
      useGpu: useGpu,
    );
    return Chat._(chat);
  }

  /// Send a message and get a stream of response tokens.
  TokenStream ask(String message) {
    return TokenStream._(_chat.ask(message));
  }

  /// Get the chat history.
  Future<List<nobodywho.Message>> getChatHistory() => _chat.getChatHistory();

  /// Set the chat history.
  Future<void> setChatHistory(List<nobodywho.Message> messages) =>
      _chat.setChatHistory(messages: messages);

  /// Reset the chat history.
  Future<void> resetHistory() => _chat.resetHistory();

  /// Reset the context with a new system prompt and tools.
  Future<void> resetContext({
    required String systemPrompt,
    required List<Tool> tools,
  }) => _chat.resetContext(
    systemPrompt: systemPrompt,
    tools: tools.map((t) => t._internalTool).toList(),
  );

  /// Set whether thinking/reasoning is allowed.
  Future<void> setAllowThinking(bool allowThinking) =>
      _chat.setAllowThinking(allowThinking: allowThinking);

  /// Set the sampler configuration.
  Future<void> setSamplerConfig(nobodywho.SamplerConfig samplerConfig) =>
      _chat.setSamplerConfig(samplerConfig: samplerConfig);

  /// Set the system prompt.
  Future<void> setSystemPrompt(String systemPrompt) =>
      _chat.setSystemPrompt(systemPrompt: systemPrompt);

  /// Set the available tools.
  Future<void> setTools(List<Tool> tools) =>
      _chat.setTools(tools: tools.map((t) => t._internalTool).toList());

  /// Stop the current generation.
  void stopGeneration() => _chat.stopGeneration();
}
