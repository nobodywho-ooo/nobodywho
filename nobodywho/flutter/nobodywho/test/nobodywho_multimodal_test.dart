@Timeout(Duration(seconds: 600))
import 'package:nobodywho/nobodywho.dart' as nobodywho;
import 'package:test/test.dart';
import 'dart:io';

void main() {
  group('Multimodal tests', () {
    final modelPath = Platform.environment["TEST_MULTIMODAL_MODEL"];
    final mmprojPath = Platform.environment["TEST_MULTIMODAL_MMPROJ"];
    final imagePath = '${Directory.current.path}/test/dog.png';

    setUpAll(() async {
      await nobodywho.NobodyWho.init();
    });

    test('askWithPrompt with text only', () async {
      if (modelPath == null) return;

      final chat = await nobodywho.Chat.fromPath(
        modelPath: modelPath,
        imageIngestion: mmprojPath,
        systemPrompt: "",
        contextSize: 2048,
        allowThinking: false,
      );

      final prompt = nobodywho.Prompt([
        nobodywho.TextPart("What is the capital of France?"),
      ]);

      final response = await chat.askWithPrompt(prompt).completed();
      expect(response, contains("Paris"));
    });

    test('askWithPrompt with image and text', () async {
      if (modelPath == null || mmprojPath == null) return;

      final chat = await nobodywho.Chat.fromPath(
        modelPath: modelPath,
        imageIngestion: mmprojPath,
        systemPrompt: "",
        contextSize: 4096,
        allowThinking: false,
      );

      final prompt = nobodywho.Prompt([
        nobodywho.TextPart(
          "Describe what animal is in this image in one word. Do not focus on the age of the animal.",
        ),
        nobodywho.ImagePart(imagePath),
      ]);

      final response = await chat.askWithPrompt(prompt).completed();
      expect(response.toLowerCase(), contains("dog"));
    });
  });
}
