@Timeout(Duration(seconds: 600))
// TODO ^ kind a sucks that we need this high a timeout
//      the issue is mostly that the llvmpipe stuff we're doing inside nix sandbox is slow as hell

import 'package:nobodywho_dart/nobodywho_dart.dart' as nobodywho;
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
    nobodywho.Chat? chat;

    setUpAll(() async {
      await nobodywho.RustLib.init();
      nobodywho.initDebugLog();
    });

    setUp(() async {
      // Additional setup goes here.
      final sparklify_tool = nobodywho.describeTool(
        function: sparklify,
        name: "sparklify",
        description: "Applies the sparklify effect to a string"
      );
      final strongify_tool = nobodywho.describeTool(
        function: strongify,
        name: "strongify",
        description: "Applies the strongify effect to a string"
      );

      chat = nobodywho.Chat.fromPath(modelPath: modelPath, systemPrompt: "", contextSize: 1024, allowThinking: false, tools: [sparklify_tool, strongify_tool]);
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
      await chat!.ask(message: "Hello!").completed();
      final messages = await chat!.getChatHistory();
      print(messages);
      expect(messages.length, equals(2));
      // TODO: test content of messages
    });

    test('Tools work with custom sampler', () async {
      final sampler = nobodywho.SamplerBuilder().topP(topP: 0.9, minKeep: 20).temperature(temperature: 1.2).dist();
      await chat!.setSamplerConfig(samplerConfig: sampler);
      final response = await chat!.ask(message: "Can you please strongify the string 'Wrawr'?").completed();

      expect(response, contains("WOW Wrawr WOW"));
    });

    test('Tools work with sampler presets', () async {
      final sampler = nobodywho.SamplerPresets.temperature(temperature: 1.2);
      await chat!.setSamplerConfig(samplerConfig: sampler);
      final response = await chat!.ask(message: "Can you please strongify the string 'Wrawr'?").completed();

      expect(response, contains("WOW Wrawr WOW"));
    });

    test('Sampler actually affects output', () async {
      // Test that greedy sampler gives deterministic output
      final greedy = nobodywho.SamplerPresets.greedy();
      await chat!.setSamplerConfig(samplerConfig: greedy);

      final response1 = await chat!.ask(message: "Say exactly: 'Hello'").completed();
      await chat!.resetHistory();
      final response2 = await chat!.ask(message: "Say exactly: 'Hello'").completed();

      expect(response1, equals(response2));  // Should be identical with greedy
    });

    test('Cosine similarity works', () {
      // Test with simple vectors
      final vec1 = [1.0, 2.0, 3.0];
      final vec2 = [4.0, 5.0, 6.0];

      final similarity = nobodywho.cosineSimilarity(a : vec1, b : vec2);

      // Check return type
      expect(similarity, isA<double>());

      // Cosine similarity should be between -1 and 1
      expect(similarity, greaterThanOrEqualTo(-1.0));
      expect(similarity, lessThanOrEqualTo(1.0));

      // Test self-similarity (should be 1.0)
      final selfSim = nobodywho.cosineSimilarity(a : vec1, b : vec1);
      expect(selfSim, closeTo(1.0, 0.001));

      // Test orthogonal vectors (should be close to 0)
      final orthogonal1 = [1.0, 0.0, 0.0];
      final orthogonal2 = [0.0, 1.0, 0.0];
      final orthogonalSim = nobodywho.cosineSimilarity(a : orthogonal1, b : orthogonal2);
      expect(orthogonalSim, closeTo(0.0, 0.001));

      // Test opposite vectors (should be close to -1)
      final opposite1 = [1.0, 2.0, 3.0];
      final opposite2 = [-1.0, -2.0, -3.0];
      final oppositeSim = nobodywho.cosineSimilarity(a : opposite1, b : opposite2);
      expect(oppositeSim, closeTo(-1.0, 0.001));
    });

    test('set_tools changes available tools', () async {
      // Create a chat with only sparklify tool
      final sparklify_tool = nobodywho.describeTool(
        function: sparklify,
        name: "sparklify",
        description: "Applies the sparklify effect to a string"
      );

      final testChat = nobodywho.Chat.fromPath(
        modelPath: modelPath,
        systemPrompt: "",
        contextSize: 1024,
        allowThinking: false,
        tools: [sparklify_tool]
      );

      // Verify sparklify tool works
      final response1 = await testChat.ask(message: "Please sparklify the word 'hello'").completed();
      expect(response1, contains("✨hello✨"));

      // Change tools to strongify
      final strongify_tool = nobodywho.describeTool(
        function: strongify,
        name: "strongify",
        description: "Applies the strongify effect to a string"
      );

      await testChat.setTools(tools: [strongify_tool]);

      // Reset history but keep new tools
      await testChat.resetHistory();

      // Verify strongify tool now works
      final response2 = await testChat.ask(message: "Please strongify the word 'test'").completed();
      expect(response2, contains("WOW test WOW"));
    });
  });
}
