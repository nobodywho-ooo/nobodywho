import 'package:nobodywho_dart/nobodywho_dart.dart';
import 'package:test/test.dart';
import 'dart:io';

void main() {
  group('A group of tests', () {
    final modelPath = Platform.environment["TEST_MODEL"]!;
    NobodyWhoModel? model;
    NobodyWhoChat? chat;

    setUp(() async {
      // Additional setup goes here.
      await RustLib.init();
      model = NobodyWhoModel(modelPath: modelPath, useGpu: false);
      chat = NobodyWhoChat(model: model!, systemPrompt: "", contextSize: 1024, tools: []);
    });

    test('First Test', () async {
      final responseStream = chat!.say(message: "What is the capital of Denmark?");
      String response = "";
      await for (final token in responseStream) {
        response += token;
      }
      expect(response, contains("Copenhagen"));
    });
  });
}
