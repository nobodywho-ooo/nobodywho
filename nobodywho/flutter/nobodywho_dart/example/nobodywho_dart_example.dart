import 'package:nobodywho_dart/nobodywho_dart.dart';

Future<void> main() async {
  await RustLib.init();
  // TODO: fix harcoded path
  final model = NobodyWhoModel(modelPath: "/home/asbjorn/Development/am/nobodywho-rs/models/Qwen2.5-7B-Instruct-Q6_K_L.gguf", useGpu: true);
  final chat = NobodyWhoChat(
      model: model,
      systemPrompt: "You are a helpful assistant.",
      contextSize: 1024,
      tools: [],
  );

  final responseStream = await chat.ask(message: "Hello, friend-o!");

  await for (final token in responseStream) {
    print(token);
  }
}
