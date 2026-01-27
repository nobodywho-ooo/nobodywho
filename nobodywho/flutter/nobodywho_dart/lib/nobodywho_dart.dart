library;

import 'dart:convert';

export 'src/rust/api/nobodywho.dart';
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
      json.decode(nobodywho.toolCallArgumentsJson(toolCall: this)) as Map<String, dynamic>;
}



