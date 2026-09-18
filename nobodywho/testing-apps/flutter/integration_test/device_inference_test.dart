import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:logging/logging.dart';
import 'package:nobodywho/nobodywho.dart' as nobodywho;

const modelUrl =
    'hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf';

/// Directory holding the test model if Test Lab pushed one.
final obbDir = Directory('/sdcard/Android/obb/ooo.nobodywho.nobodywho_testapp');

/// The preloaded copy if Test Lab pushed one. For local runs there won't be one,
/// so we fall back on a URL. The .obb file is matched by extension
/// rather than by name, so a versionCode bump cannot silently miss it.
String selectModelPath() {
  if (!obbDir.existsSync()) return modelUrl;
  final preloaded = obbDir.listSync().whereType<File>().where(
    (f) => f.path.endsWith('.obb'),
  );
  return preloaded.isEmpty ? modelUrl : preloaded.first.path;
}

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
      final nobodywho.Chat chat = await nobodywho.Chat.fromPath(
        modelPath: selectModelPath(),
        systemPrompt: 'Reply with one word only.',
        templateVariables: const {'enable_thinking': false},
        useGpu: true,
      );

      // Completion
      final String response = await chat.ask('Say hello').completed();
      expect(response, isNotEmpty, reason: 'completion should be non-empty');

      // Streaming
      await chat.resetContext(systemPrompt: 'Reply briefly.', tools: []);
      int tokenCount = 0;
      await for (final String _ in chat.ask('Say hi')) {
        tokenCount++;
      }
      expect(
        tokenCount,
        greaterThan(0),
        reason: 'streaming should yield at least one token',
      );

      // Tool calling. The Tool is constructed here rather than up front because
      // handing one to the Rust side consumes it — reusing a Tool across calls
      // currently throws DroppableDisposedException (NOB-168).
      final nobodywho.Tool pingTool = nobodywho.Tool(
        function: ping,
        name: 'ping',
        description: 'Ping the server',
      );
      await chat.resetContext(
        systemPrompt: 'Use the ping tool now.',
        tools: [pingTool],
      );
      await chat.ask('Ping the server').completed();

      final Iterable<nobodywho.Message_Tool> toolMessages =
          (await chat.getChatHistory()).whereType<nobodywho.Message_Tool>();
      expect(
        toolMessages,
        isNotEmpty,
        reason: 'expected a tool response in chat history',
      );
      final String toolText = toolMessages.first.content.text;
      expect(toolText, 'pong');
    },
    // The model is preloaded, so this covers inference only. Kept well clear
    // of the ~10s a passing run takes, but short enough that a fallback to
    // downloading on-device fails loudly instead of eating the FTL budget.
    timeout: const Timeout(Duration(minutes: 5)),
  );
}
