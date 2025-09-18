import 'package:flutter/material.dart';
import 'package:nobodywho_flutter/nobodywho_flutter.dart';

// this is here to be a demo tool
String sparklify({required String thestring}) {
  return "✨" + thestring + "✨";
}

Future<void> main() async {
  await RustLib.init();

  // the following code is just for testing. I need to work this into a proper app.

  // load the LLM
  final model = NobodyWhoModel(modelPath: "/home/asbjorn/Development/am/nobodywho-rs/models/Qwen_Qwen3-4B-Q4_K_M.gguf", useGpu: true);

  // initialize a tool
  final tool = toolFromFunction(
    function: sparklify,
    name: "sparklify",
    description: "Applies the sparklify effect to a given string"
  );

  // initialize a chat (w/ tool)
  final chat = NobodyWhoChat(model: model, systemPrompt: "", contextSize: 2048, tools: [tool]);

  // send a message
  final response_stream = await chat.say(message: "Can you sparklify this text: 'HELLO, WORLD!!!'");

  // stream out the response
  await for (final token in response_stream) {
    print(token);
  }

  // start my app
  runApp(const MyApp());
}

class MyApp extends StatelessWidget {
  const MyApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      home: Scaffold(
        appBar: AppBar(title: const Text('flutter_rust_bridge quickstart')),
        body: Center(
          child: Text(
            'Action: Call Rust `greet("Tom")`\nResult: `${greet(name: "Tom")}`',
          ),
        ),
      ),
    );
  }
}
