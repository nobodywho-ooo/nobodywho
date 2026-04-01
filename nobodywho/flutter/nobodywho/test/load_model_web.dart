import 'package:nobodywho/nobodywho.dart' as nobodywho;

const _modelUrl =
    'https://huggingface.co/bartowski/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf';

Future<nobodywho.Model> loadTestModel() async {
  // TODO: fetch model from _modelUrl and use Model.fromBytes()
  throw UnimplementedError('Web model loading not yet implemented');
}

Future<nobodywho.Encoder?> loadTestEncoder() async {
  // Encoder not yet supported on web
  return null;
}

Future<nobodywho.CrossEncoder?> loadTestCrossEncoder() async {
  // CrossEncoder not yet supported on web
  return null;
}
