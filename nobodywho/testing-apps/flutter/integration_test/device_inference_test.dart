import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:logging/logging.dart';
import 'package:nobodywho/nobodywho.dart' as nobodywho;

/// The model is downloaded on-device into the app's own cache on first run —
/// no permissions and no shared storage involved.
const modelUrl =
    'hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf';

String ping() => 'pong';

/// On-device smoke test, run on real hardware via Firebase Test Lab.
///
/// Mirrors the Kotlin binding's `DeviceInferenceTest`: completion, streaming
/// and tool calling, exercising the arm64 native library on a physical phone.
void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'chat completes, streams and calls tools',
    (tester) async {
      // FRB forwards Rust/llama.cpp logs to Dart logging, but the library
      // intentionally leaves output to its host. Print them into FTL logcat.
      final previousLogLevel = Logger.root.level;
      Logger.root.level = Level.ALL;
      final logSubscription = Logger.root.onRecord.listen((record) {
        // ignore: avoid_print
        print(
          '[${record.level.name}] ${record.loggerName}: ${record.message}'
          '${record.error == null ? '' : '\n${record.error}'}'
          '${record.stackTrace == null ? '' : '\n${record.stackTrace}'}',
        );
      });
      addTearDown(() async {
        await logSubscription.cancel();
        Logger.root.level = previousLogLevel;
      });
      await nobodywho.NobodyWho.init();

      // Exercise automatic OpenCL/Vulkan selection with CPU fallback.
      final chat = await nobodywho.Chat.fromPath(
        modelPath: modelUrl,
        systemPrompt: 'Reply with one word only.',
        templateVariables: const {'enable_thinking': false},
        useGpu: true,
      );

      // Completion
      final response = await chat.ask('Say hello').completed();
      expect(response, isNotEmpty, reason: 'completion should be non-empty');

      // Streaming
      await chat.resetContext(systemPrompt: 'Reply briefly.', tools: []);
      var tokenCount = 0;
      await for (final _ in chat.ask('Say hi')) {
        tokenCount++;
      }
      expect(tokenCount, greaterThan(0),
          reason: 'streaming should yield at least one token');

      // Tool calling. The Tool is constructed here rather than up front because
      // handing one to the Rust side consumes it — reusing a Tool across calls
      // currently throws DroppableDisposedException (NOB-168).
      final pingTool = nobodywho.Tool(
        function: ping,
        name: 'ping',
        description: 'Ping the server',
      );
      await chat.resetContext(
        systemPrompt: 'Use the ping tool now.',
        tools: [pingTool],
      );
      await chat.ask('Ping the server').completed();

      final toolMessages =
          (await chat.getChatHistory()).whereType<nobodywho.Message_Tool>();
      expect(toolMessages, isNotEmpty,
          reason: 'expected a tool response in chat history');
      expect(toolMessages.first.content.text, 'pong');
    },
    // Generous: the first run downloads the model before any inference starts.
    timeout: const Timeout(Duration(minutes: 20)),
  );
}
