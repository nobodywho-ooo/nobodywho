@Timeout(Duration(seconds: 600))
// TODO ^ kind a sucks that we need this high a timeout
//      the issue is mostly that the llvmpipe stuff we're doing inside nix sandbox is slow as hell

import 'package:nobodywho_dart/nobodywho_dart.dart';
import 'package:test/test.dart';
import 'dart:io';

String sparklify({required String text}) {
  print("Sparklify called!");
  return '✨$text✨';
}

// the name of this tool doesn't really make sense
// but that helps is ensure it was called
Future<String> strongify({required String text}) async {
  print("Strongify called!");
  return 'WOW $text WOW';
}

void main() {
  group('A group of tests', () {
    final modelPath = Platform.environment["TEST_MODEL"]!;
    NobodyWhoModel? model;
    NobodyWhoChat? chat;
    NobodyWhoTool? tool;

    setUpAll(() async {
      await RustLib.init();
      initDebugLog();
    });

    setUp(() async {
      // Additional setup goes here.
      final sparklify_tool = describeTool(
        function: sparklify,
        name: "sparklify",
        description: "Applies the sparklify effect to a string"
      );
      final strongify_tool = describeTool(
        function: strongify,
        name: "strongify",
        description: "Applies the strongify effect to a string"
      );

      model = NobodyWhoModel(modelPath: modelPath, useGpu: false);
      chat = NobodyWhoChat(model: model!, systemPrompt: "", contextSize: 1024, tools: [sparklify_tool, strongify_tool]);
    });

    test('Capital of Denmark test', () async {
      final responseStream = chat!.ask(message: "What is the capital of Denmark?");
      String response = "";
      await for (final token in responseStream.iter()) {
        response += token;
      }
      expect(response, contains("Copenhagen"));
    });

    test('Tool calling test', () async {
      final responseStream = chat!.ask(message: "Can you please sparklify the string 'Foopdoop'?");
      String response = "";
      await for (final token in responseStream.iter()) {
        response += token;
      }
      expect(response, contains("✨Foopdoop✨"));
    });

    test('Async tool calling test', () async {
      final responseStream = chat!.ask(message: "Can you please strongify the string 'Wrawr'?");
      String response = "";
      await for (final token in responseStream.iter()) {
        response += token;
      }

      expect(response, contains("WOW Wrawr WOW"));
    });

    test('Get chat history', () async {
      final messages = await chat!.get_chat_history();
      print(messages);
      expect(messages.length, equals(2));
    });
  });
}
