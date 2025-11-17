import 'package:nobodywho_dart/nobodywho_dart.dart';
import 'package:test/test.dart';
import 'dart:io';

String capitalize({required String text}) {
  return text.toUpperCase();
}

void main() {
  group('A group of tests', () {
    final modelPath = Platform.environment["TEST_MODEL"]!;
    NobodyWhoModel? model;
    NobodyWhoChat? chat;
    NobodyWhoTool? tool;

    setUpAll(() async {
      await RustLib.init();
    });

    setUp(() async {
      // Additional setup goes here.
      tool = describeTool(function: capitalize, name: "capitalize", description: "Takes a string and returns the same string capitalized.");
      model = NobodyWhoModel(modelPath: modelPath, useGpu: false);
      chat = NobodyWhoChat(model: model!, systemPrompt: "", contextSize: 1024, tools: [tool!]);
    });

    test('Capital of Denmark test', () async {
      final responseStream = chat!.say(message: "What is the capital of Denmark?");
      String response = "";
      await for (final token in responseStream) {
        response += token;
      }
      expect(response, contains("Copenhagen"));
    });

    test('Tool calling test', () async {
      final responseStream = chat!.say(message: "What is the capital of Denmark?");
      String response = "";
      await for (final token in responseStream) {
        response += token;
      }
      expect(response, contains("Copenhagen"));
    });
  });
}
