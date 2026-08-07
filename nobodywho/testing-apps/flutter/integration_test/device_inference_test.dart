import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
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
      await nobodywho.NobodyWho.init();

      // Ask for the GPU like a real app would. Android has no GPU backend yet,
      // so this falls back to CPU today and starts exercising the GPU path
      // automatically once one lands.
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
      expect(toolMessages.first.content, 'pong');
    },
    // Generous: the first run downloads the model before any inference starts.
    timeout: const Timeout(Duration(minutes: 20)),
  );
}
