@Timeout(Duration(seconds: 600))
// TODO ^ kind a sucks that we need this high a timeout
//      the issue is mostly that the llvmpipe stuff we're doing inside nix sandbox is slow as hell
import 'package:nobodywho/nobodywho.dart' as nobodywho;
import 'package:test/test.dart';
import 'dart:io';
// Mock ToolCall for testing - we can't easily create real ones without Rust
// so we'll test Message construction patterns that don't require ToolCall

List<List<double>> multiplyMatrices(List<List<double>> a, List<List<double>> b) {
    final rowsA = a.length;
    final colsA = a[0].length;
    final colsB = b[0].length;

    // Initialize result matrix with zeros
    final result = List.generate(rowsA, (_) => List.filled(colsB, 0.0));

    for (var i = 0; i < rowsA; i++) {
      for (var j = 0; j < colsB; j++) {
        for (var k = 0; k < colsA; k++) {
          result[i][j] += a[i][k] * b[k][j];
        }
      }
    }

    return result;
  }



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



String addListOfVectors({required List<List<int>> listOfVectors}) {
  List<int> sum = listOfVectors.reduce(
    (acc, vec) => [for (int i = 0; i < acc.length; i++) acc[i] + vec[i]],
  );
  return sum.toString();
}

String noArgs() {
  print("noArgs called!");
  return "This function has no args!";
}


String mutliplySquareMatrices({required List<List<List<double>>> listOfMatrices}){
  return listOfMatrices.reduce((accu, curr) => multiplyMatrices(accu,curr)).toString();
}


String addTwoNumbers({required double a, required double b}){
  return (a + b).toString();
}

void main() {
  group('A group of tests', () {
    final modelPath = Platform.environment["TEST_MODEL"]!;
    nobodywho.Chat? chat;

    setUpAll(() async {
      await nobodywho.NobodyWho.init();
    });

    setUp(() async {
      // Additional setup goes here.
      final sparklify_tool = nobodywho.Tool(
        function: sparklify,
        name: "sparklify",
        description: "Applies the sparklify effect to a string",
        parameterDescriptions: {"text" : "The text to be sparklified."},
      );
      final strongify_tool = nobodywho.Tool(
        function: strongify,
        name: "strongify",
        description: "Applies the strongify effect to a string",
      );

      final noargs_tool = nobodywho.Tool(
        function : noArgs,
        name: "noArgs",
        description: "This function does nothing and returns some string",
      );

      final addTwoNumbersTool = nobodywho.Tool(
        function : addTwoNumbers,
        name : "addTwoNumbers",
        description : "This function adds up the two numbers.",
      );

      final addListOfVectorsTool = nobodywho.Tool(
        function : addListOfVectors,
        name : "addListOfVectors",
        description : "Does vector addition on the list of vectors provided",
        parameterDescriptions : {"listOfVectors" : "List of vectors to added. Each vector must of same length"},
      );

      final multiplySquareMatricesTool = nobodywho.Tool(
        function : mutliplySquareMatrices,
        name : "multiplySquareMatrices",
        description : "Multiplies all the matrices in list together.",
        parameterDescriptions : {"listOfMatrices" : "List of matrices to multiply. Each matrix must be a square matrix and they must all be of the same dimension."},
      );

      chat = await nobodywho.Chat.fromPath(
        modelPath: modelPath,
        systemPrompt: "",
        contextSize: 2048,
        allowThinking: false,
        tools: [sparklify_tool, strongify_tool, noargs_tool, addTwoNumbersTool, addListOfVectorsTool, multiplySquareMatricesTool],
      );
    });

    test('Capital of Denmark test', () async {
      final responseStream = chat!.ask("What is the capital of Denmark?");
      String response = "";
      await for (final token in responseStream) {
        response += token;
      }
      expect(response, contains("Copenhagen"));
    });

    test('Tool calling test', () async {
      final responseStream = chat!.ask(
        "Can you please sparklify the string 'Foopdoop'?",
      );
      String response = "";
      await for (final token in responseStream) {
        response += token;
      }
      expect(response, contains("✨Foopdoop✨"));
    });

    test('Async tool calling test', () async {
      final responseStream = chat!.ask(
        "Can you please strongify the string 'Wrawr'?",
      );
      String response = "";
      await for (final token in responseStream) {
        response += token;
      }

      expect(response, contains("WOW Wrawr WOW"));
    });


    test('Tool calling with no arguments test', () async{
      final response = await chat!.ask(
        "Please make a call to the noArgs function and show me the exact output!"
      ).completed();
      final history = await chat!.getChatHistory();
      final toolCall = history.firstWhere((m) => m.role == nobodywho.Role.tool);

      expect(toolCall.content, contains("This function has no args!"));
    });

    test('Tool calling for doubles', () async{
      final response = await chat!.ask(
        "Please use the provided tool to add the numbers 13 and 17"
      ).completed();
      final history = await chat!.getChatHistory();
      final toolCall = history.firstWhere((m) => m.role == nobodywho.Role.tool);

      expect(toolCall.content, contains("30"));
    });


    test('Tool calling with 2 layer nested list arguments', () async{
      final response = await chat!.ask(
        "Please use the provided tool to add the vectors [[1,2,3],[4,5,6],[7,8,9]]."
      ).completed();
      final history = await chat!.getChatHistory();
      final toolCall = history.firstWhere((m) => m.role == nobodywho.Role.tool);

      expect(toolCall.content, contains("[12, 15, 18]"));
    });

    test('Tool calling with 3 layer nested list arguments', () async{
      final response = await chat!.ask(
        "Please use the provided tool multiply the matrices [[1, 2],[3, 5]], [[1, 3],[13, 4]] and [[-5, 2],[23, 3]]."
      ).completed();
      final history = await chat!.getChatHistory();
      final toolCall = history.firstWhere((m) => m.role == nobodywho.Role.tool);

      expect(toolCall.content, contains("[[118.0, 87.0], [327.0, 223.0]]"));
    });


    test('Get chat history', () async {
      await chat!.ask("Hello!").completed();
      final messages = await chat!.getChatHistory();
      print(messages);
      expect(messages.length, equals(2));
      // TODO: test content of messages
    });

    test('Tool call message has correct structure in chat history', () async {
      // Trigger a tool call
      await chat!.ask("Please sparklify the word 'test'").completed();

      final messages = await chat!.getChatHistory();

      // Find the tool calls message (assistant requesting tool call)
      final toolCallsMessage =
          messages.firstWhere(
                (m) => m is nobodywho.Message_ToolCalls,
                orElse: () =>
                    throw Exception('No tool calls message found in history'),
              )
              as nobodywho.Message_ToolCalls;

      // Check the role is assistant
      expect(toolCallsMessage.role, equals(nobodywho.Role.assistant));

      // Check there's at least one tool call
      expect(toolCallsMessage.toolCalls, isNotEmpty);

      // Get the first tool call and verify its properties
      final toolCall = toolCallsMessage.toolCalls.first;
      expect(toolCall.name, equals('sparklify'));

      // The arguments should contain the text parameter
      // Note: arguments is a serde_json::Value, we need to check it's not null
      expect(toolCall.arguments, isNotNull);

      // Test the argumentsJson getter - should return valid JSON string
      final argsJson = toolCall.argumentsJson;
      expect(argsJson, isA<String>());
      expect(argsJson, contains('text'));

      // Test the argumentsMap getter - should return parsed Map
      final argsMap = toolCall.argumentsMap;
      expect(argsMap, isA<Map<String, dynamic>>());
      expect(argsMap.containsKey('text'), isTrue);
      expect(argsMap['text'], equals('test'));

      // Find the tool response message
      final toolRespMessage =
          messages.firstWhere(
                (m) => m is nobodywho.Message_ToolResp,
                orElse: () => throw Exception(
                  'No tool response message found in history',
                ),
              )
              as nobodywho.Message_ToolResp;

      // Check the tool response
      expect(toolRespMessage.role, equals(nobodywho.Role.tool));
      expect(toolRespMessage.name, equals('sparklify'));
      expect(toolRespMessage.content, contains('✨test✨'));
    });

    test('Tools work with custom sampler', () async {
      final sampler = nobodywho.SamplerBuilder()
          .topP(topP: 0.9, minKeep: 20)
          .temperature(temperature: 1.2)
          .dist();
      await chat!.setSamplerConfig(sampler);
      final response = await chat!
          .ask("Can you please strongify the string 'Wrawr'?")
          .completed();

      expect(response, contains("WOW Wrawr WOW"));
    });

    test('Tools work with sampler presets', () async {
      final sampler = nobodywho.SamplerPresets.temperature(temperature: 1.2);
      await chat!.setSamplerConfig(sampler);
      final response = await chat!
          .ask("Can you please strongify the string 'Wrawr'?")
          .completed();

      expect(response, contains("WOW Wrawr WOW"));
    });

    test('Sampler actually affects output', () async {
      // Test that greedy sampler gives deterministic output
      final greedy = nobodywho.SamplerPresets.greedy();
      await chat!.setSamplerConfig(greedy);

      final response1 = await chat!.ask("Say exactly: 'Hello'").completed();
      await chat!.resetHistory();
      final response2 = await chat!.ask("Say exactly: 'Hello'").completed();

      expect(response1, equals(response2)); // Should be identical with greedy
    });

    test('Cosine similarity works', () {
      // Test with simple vectors
      final vec1 = [1.0, 2.0, 3.0];
      final vec2 = [4.0, 5.0, 6.0];

      final similarity = nobodywho.cosineSimilarity(a: vec1, b: vec2);

      // Check return type
      expect(similarity, isA<double>());

      // Cosine similarity should be between -1 and 1
      expect(similarity, greaterThanOrEqualTo(-1.0));
      expect(similarity, lessThanOrEqualTo(1.0));

      // Test self-similarity (should be 1.0)
      final selfSim = nobodywho.cosineSimilarity(a: vec1, b: vec1);
      expect(selfSim, closeTo(1.0, 0.001));

      // Test orthogonal vectors (should be close to 0)
      final orthogonal1 = [1.0, 0.0, 0.0];
      final orthogonal2 = [0.0, 1.0, 0.0];
      final orthogonalSim = nobodywho.cosineSimilarity(
        a: orthogonal1,
        b: orthogonal2,
      );
      expect(orthogonalSim, closeTo(0.0, 0.001));

      // Test opposite vectors (should be close to -1)
      final opposite1 = [1.0, 2.0, 3.0];
      final opposite2 = [-1.0, -2.0, -3.0];
      final oppositeSim = nobodywho.cosineSimilarity(
        a: opposite1,
        b: opposite2,
      );
      expect(oppositeSim, closeTo(-1.0, 0.001));
    });

    test('set_tools changes available tools', () async {
      // Create a chat with only sparklify tool
      final sparklify_tool = nobodywho.Tool(
        function: sparklify,
        name: "sparklify",
        description: "Applies the sparklify effect to a string",
      );

      final testChat = await nobodywho.Chat.fromPath(
        modelPath: modelPath,
        systemPrompt: "",
        contextSize: 1024,
        allowThinking: false,
        tools: [sparklify_tool],
      );

      // Verify sparklify tool works
      final response1 = await testChat
          .ask("Please sparklify the word 'hello'")
          .completed();
      expect(response1, contains("✨hello✨"));

      // Change tools to strongify
      final strongify_tool = nobodywho.Tool(
        function: strongify,
        name: "strongify",
        description: "Applies the strongify effect to a string",
      );

      await testChat.setTools([strongify_tool]);

      // Reset history but keep new tools
      await testChat.resetHistory();

      // Verify strongify tool now works
      final response2 = await testChat
          .ask("Please strongify the word 'test'")
          .completed();
      expect(response2, contains("WOW test WOW"));
    });


    test('Encoder.fromPath creates encoder and encodes text', () async {
      final modelPath = Platform.environment["TEST_EMBEDDINGS_MODEL"];
      if (modelPath == null) {
        return; // Skip test if model not provided
      }

      final encoder = await nobodywho.Encoder.fromPath(modelPath: modelPath);
      final embeddings = await encoder.encode(text: "Test text for embedding.");

      // Basic checks
      expect(embeddings, isA<List<double>>());
      expect(embeddings.length, greaterThan(0));
      expect(embeddings.every((x) => x is double), isTrue);

      // Verify self-similarity is close to 1.0 (embeddings make sense)
      final selfSim = nobodywho.cosineSimilarity(a: embeddings, b: embeddings);
      expect(selfSim, closeTo(1.0, 0.001));
    });

    test('CrossEncoder.fromPath creates cross encoder and ranks documents', () async {
      final modelPath = Platform.environment["TEST_CROSSENCODER_MODEL"];
      if (modelPath == null) {
        return; // Skip test if model not provided
      }

      final crossEncoder = await nobodywho.CrossEncoder.fromPath(modelPath: modelPath);
      final documents = [
        "Paris is the capital of France.",
        "Berlin is the capital of Germany.",
        "The weather is nice today.",
      ];

      final scores = await crossEncoder.rank(
        query: "What is the capital of France?",
        documents: documents,
      );

      // Basic checks
      expect(scores, isA<List<double>>());
      expect(scores.length, equals(documents.length));
      expect(scores.every((x) => x is double), isTrue);

      // Verify most relevant document has highest score
      expect(scores[0], greaterThan(scores[1]));
      expect(scores[0], greaterThan(scores[2]));
    });

  });

  group('Message struct tests', () {
    test('Message.message constructor creates correct instance', () {
      final msg = nobodywho.Message.message(
        role: nobodywho.Role.user,
        content: 'Hello, world!',
      );

      expect(msg, isA<nobodywho.Message_Message>());
      expect(
        (msg as nobodywho.Message_Message).role,
        equals(nobodywho.Role.user),
      );
      expect(msg.content, equals('Hello, world!'));
    });

    test('Message.message with different roles', () {
      final userMsg = nobodywho.Message.message(
        role: nobodywho.Role.user,
        content: 'User message',
      );
      final assistantMsg = nobodywho.Message.message(
        role: nobodywho.Role.assistant,
        content: 'Assistant message',
      );
      final systemMsg = nobodywho.Message.message(
        role: nobodywho.Role.system,
        content: 'System message',
      );

      expect(
        (userMsg as nobodywho.Message_Message).role,
        equals(nobodywho.Role.user),
      );
      expect(
        (assistantMsg as nobodywho.Message_Message).role,
        equals(nobodywho.Role.assistant),
      );
      expect(
        (systemMsg as nobodywho.Message_Message).role,
        equals(nobodywho.Role.system),
      );
    });

    test('Message.message with empty content', () {
      final msg = nobodywho.Message.message(
        role: nobodywho.Role.assistant,
        content: '',
      );

      expect((msg as nobodywho.Message_Message).content, equals(''));
    });

    test('Message.message with multiline content', () {
      final content = 'Line 1\nLine 2\nLine 3';
      final msg = nobodywho.Message.message(
        role: nobodywho.Role.user,
        content: content,
      );

      expect((msg as nobodywho.Message_Message).content, equals(content));
    });

    test('Message.message with special characters', () {
      final content = 'Hello! 🎉 Special chars: <>&"\'';
      final msg = nobodywho.Message.message(
        role: nobodywho.Role.user,
        content: content,
      );

      expect((msg as nobodywho.Message_Message).content, equals(content));
    });

    test('Message.toolResp constructor creates correct instance', () {
      final msg = nobodywho.Message.toolResp(
        role: nobodywho.Role.tool,
        name: 'calculator',
        content: '42',
      );

      expect(msg, isA<nobodywho.Message_ToolResp>());
      expect(
        (msg as nobodywho.Message_ToolResp).role,
        equals(nobodywho.Role.tool),
      );
      expect(msg.name, equals('calculator'));
      expect(msg.content, equals('42'));
    });

    test('Message.toolResp with JSON content', () {
      final jsonContent = '{"result": 42, "status": "success"}';
      final msg = nobodywho.Message.toolResp(
        role: nobodywho.Role.tool,
        name: 'api_call',
        content: jsonContent,
      );

      expect((msg as nobodywho.Message_ToolResp).content, equals(jsonContent));
    });

    test('Role enum has all expected values', () {
      expect(nobodywho.Role.values, contains(nobodywho.Role.user));
      expect(nobodywho.Role.values, contains(nobodywho.Role.assistant));
      expect(nobodywho.Role.values, contains(nobodywho.Role.system));
      expect(nobodywho.Role.values, contains(nobodywho.Role.tool));
      expect(nobodywho.Role.values.length, equals(4));
    });

    test('Message variants are distinguishable', () {
      final textMsg = nobodywho.Message.message(
        role: nobodywho.Role.user,
        content: 'Hello',
      );
      final toolRespMsg = nobodywho.Message.toolResp(
        role: nobodywho.Role.tool,
        name: 'test_tool',
        content: 'result',
      );

      expect(textMsg, isA<nobodywho.Message_Message>());
      expect(textMsg, isNot(isA<nobodywho.Message_ToolResp>()));
      expect(toolRespMsg, isA<nobodywho.Message_ToolResp>());
      expect(toolRespMsg, isNot(isA<nobodywho.Message_Message>()));
    });

    test('Message equality works correctly', () {
      final msg1 = nobodywho.Message.message(
        role: nobodywho.Role.user,
        content: 'Hello',
      );
      final msg2 = nobodywho.Message.message(
        role: nobodywho.Role.user,
        content: 'Hello',
      );
      final msg3 = nobodywho.Message.message(
        role: nobodywho.Role.user,
        content: 'Different',
      );

      expect(msg1, equals(msg2));
      expect(msg1, isNot(equals(msg3)));
    });

    test('Message hashCode is consistent', () {
      final msg1 = nobodywho.Message.message(
        role: nobodywho.Role.user,
        content: 'Hello',
      );
      final msg2 = nobodywho.Message.message(
        role: nobodywho.Role.user,
        content: 'Hello',
      );

      expect(msg1.hashCode, equals(msg2.hashCode));
    });

    test('Message can be used in collections', () {
      final messages = <nobodywho.Message>[
        nobodywho.Message.message(
          role: nobodywho.Role.system,
          content: 'You are helpful',
        ),
        nobodywho.Message.message(role: nobodywho.Role.user, content: 'Hi'),
        nobodywho.Message.message(
          role: nobodywho.Role.assistant,
          content: 'Hello!',
        ),
      ];

      expect(messages.length, equals(3));
      expect(messages[0], isA<nobodywho.Message_Message>());
      expect(
        (messages[1] as nobodywho.Message_Message).role,
        equals(nobodywho.Role.user),
      );
    });

    test('Message copyWith works for Message_Message', () {
      final original =
          nobodywho.Message.message(
                role: nobodywho.Role.user,
                content: 'Original',
              )
              as nobodywho.Message_Message;

      final modified = original.copyWith(content: 'Modified');

      expect(modified.role, equals(nobodywho.Role.user));
      expect(modified.content, equals('Modified'));
      expect(original.content, equals('Original')); // Original unchanged
    });

    test('Message copyWith works for Message_ToolResp', () {
      final original =
          nobodywho.Message.toolResp(
                role: nobodywho.Role.tool,
                name: 'original_tool',
                content: 'result',
              )
              as nobodywho.Message_ToolResp;

      final modified = original.copyWith(name: 'new_tool');

      expect(modified.role, equals(nobodywho.Role.tool));
      expect(modified.name, equals('new_tool'));
      expect(modified.content, equals('result'));
      expect(original.name, equals('original_tool')); // Original unchanged
    });
  });

}
