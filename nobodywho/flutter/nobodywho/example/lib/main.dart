import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:nobodywho/nobodywho.dart' as nobodywho;
import 'package:path_provider/path_provider.dart';

void main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await nobodywho.NobodyWho.init();

  runApp(const MainApp());
}

class MainApp extends StatelessWidget {
  const MainApp({super.key});

  Future<void> _onPressed() async {
    try {
      final dir = await getApplicationDocumentsDirectory();
      // Put this mode in assets folder : https://huggingface.co/bartowski/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf
      // Rename it model.gguf
      final model = File('${dir.path}/model.gguf');
      // Remember to register assets folder in pubspec.yaml like so :
      // flutter:
      //   assets:
      //     - assets/

      if (!await model.exists()) {
        final data = await rootBundle.load('assets/model.gguf');
        await model.writeAsBytes(data.buffer.asUint8List(), flush: true);
      }

      final chat = await nobodywho.Chat.fromPath(modelPath: model.path);
      final msg = await chat.ask('Is water wet?').completed();

      print(msg);
    } catch (err) {
      print("Error :$err");
    }
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      home: Scaffold(
        body: Center(
          child: ElevatedButton(
            onPressed: _onPressed,
            child: Text("Ask question"),
          ),
        ),
      ),
    );
  }
}
