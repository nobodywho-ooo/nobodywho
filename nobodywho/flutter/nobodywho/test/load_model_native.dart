import 'dart:io';
import 'package:nobodywho/nobodywho.dart' as nobodywho;

Future<nobodywho.Model> loadTestModel() async {
  final modelPath = Platform.environment["TEST_MODEL"]!;
  return nobodywho.Model.load(modelPath: modelPath);
}

Future<nobodywho.Encoder?> loadTestEncoder() async {
  final modelPath = Platform.environment["TEST_EMBEDDINGS_MODEL"];
  if (modelPath == null) return null;
  return nobodywho.Encoder.fromPath(modelPath: modelPath);
}

Future<nobodywho.CrossEncoder?> loadTestCrossEncoder() async {
  final modelPath = Platform.environment["TEST_CROSSENCODER_MODEL"];
  if (modelPath == null) return null;
  return nobodywho.CrossEncoder.fromPath(modelPath: modelPath);
}
