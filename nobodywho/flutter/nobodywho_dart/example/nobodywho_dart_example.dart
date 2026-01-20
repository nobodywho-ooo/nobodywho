import 'package:nobodywho_dart/nobodywho_dart.dart' as nobodywho;

Future<void> main() async {
  await nobodywho.NobodyWho.init();
  // TODO: fix harcoded path
  final model = await nobodywho.Model.load(
    modelPath:
        "/home/asbjorn/Development/am/nobodywho-rs/models/Qwen2.5-7B-Instruct-Q6_K_L.gguf",
    useGpu: true,
  );
  final chat = nobodywho.Chat(
    model: model,
    systemPrompt: "You are a helpful assistant.",
    contextSize: 1024,
    tools: [],
  );

  final responseStream = chat.ask(message: "Hello, friend-o!");

  await for (final token in responseStream.iter()) {
    print(token);
  }
}
