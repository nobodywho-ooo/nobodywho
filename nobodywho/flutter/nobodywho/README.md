# NobodyWho Flutter

NobodyWho is a flutter plugin for running large language models locally and offline. 

Free to use in commercial project under EUPL-1.2 license, no API key needed.

- [Documentation](https://docs.nobodywho.ooo) - Flutter & others frameworks documentation
- [Discord](https://discord.gg/qhaMc2qCYB) — Get help, share ideas, connect with other developers
- [GitHub Issues](https://github.com/nobodywho-ooo/nobodywho/issues) — Report bugs
- [GitHub Discussions](https://github.com/nobodywho-ooo/nobodywho/discussions) — Ask questions, request features

## Quick Start

Run the following to install the library :

`flutter pub add nobodywho`

In your `main.dart`, import the engine :

```
import 'package:nobodywho/nobodywho.dart' as nobodywho;

void main() async {
  WidgetsFlutterBinding.ensureInitialized();

  await nobodywho.NobodyWho.init();

  runApp(const MyApp());
}
```

### Model

Nobody engine support the GGUF model, which is a binary file designed for fast loading and efficient inference of LLMs. You can find many models on [Hugging Face](https://huggingface.co/models) to download, but let's keep that for later.

**Note**: Not all of GGUF models will work. Some won't work because they are incorrectly formatted. Also make sure that the model size is compatible with the device that will run the model. On mobile, any model under 1 GB should run perfectly. As a rule of thumb, ensure the device has at least twice the RAM of the model file size.

Then you need a model. Either you let the user decide which model he wants to use and provide a list of models he can download using [background_downloader](https://pub.dev/packages/background_downloader) for e.g or you can add a model in the `assets` folder.

Having a large model in your `assets` folder isn't ideal to keep the size of your app small, but to keep this tutorial short or for you to quickly play around with NobodyWho library, we will have a small model in the `assets` folder. You can go to the next section if you don't want a model in the asset folder.

First, make sure that flutter can access your files in the `assets` folder in the `pubspec.yaml` file:

```
flutter:
  assets:
    - assets/
```

Then download a compatible GGUF model, like this [one](https://huggingface.co/bartowski/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf), rename it `model.gguf` and put in inside the `assets` folder.

In order to access files in the `assets` folder, we need `path_provider`. Add it with the following command :

`flutter pub add path_provider`

Let's get the path of our model :

```dart
import 'dart:io';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';
import 'package:nobodywho/nobodywho.dart' as nobodywho;

...

final dir = await getApplicationDocumentsDirectory();
final model = File('${dir.path}/model.gguf');

if (!await model.exists()) {
  final data = await rootBundle.load('assets/model.gguf');
  await model.writeAsBytes(data.buffer.asUint8List(), flush: true);
}
```

Now you can load the model in your app using `model.path`.

### Chat

You can configure your chat to improve its accuracy, add chat history, system prompt and enhance cognitive performance. You can find out more about this in the [documentation](https://docs.nobodywho.ooo/flutter/chat/), but here is an example :

```dart
import 'package:path_provider/path_provider.dart';

...

final chat = await nobodywho.Chat.fromPath(modelPath: model.path); // replace model.path with your model's path
final msg = await chat.ask('Is water wet?').completed();
print(msg); // Yes, indeed, water is wet!
```

### Tool Calling

Give your LLM the ability to interact with the outside world, here is an example :

```dart
import 'dart:math' as math;
import 'package:nobodywho_dart/nobodywho_dart.dart' as nobodywho;

final circleAreaTool = nobodywho.Tool(
  name: "circle_area",
  description: "Calculates the area of a circle given its radius",
  function: ({ required double radius }) {
    final area = math.pi * radius * radius;
    return "Circle with radius $radius has area ${area.toStringAsFixed(2)}";
  }
);
```

To let your LLM use it, simply add it when creating Chat:

```dart
final chat = nobodywho.Chat.fromPath(
  modelPath: './model.gguf',
  tools: [circleAreaTool]
);
```

Then ask a question related to the tool :

```dart
final response = await chat.ask('What is the area of a circle with a radius of 2?').completed();
print(response);
```

Tool calling documentation can be found [here](https://docs.nobodywho.ooo/flutter/tool-calling/).

### Sampling

The model does not produce tokens but rather a probability distribution over all possible tokens. We must then choose how to pick the next token from the distribution. This is the job of a sampler, which using NobodyWho you can freely modify, to achieve better quality outputs or constrain the outputs to some known format (e.g. JSON).

```dart
import 'package:nobodywho_dart/nobodywho_dart.dart' as nobodywho;

final chat = await nobodywho.Chat.fromPath(
  modelPath: "./model.gguf",
  sampler: nobodywho.SamplerPresets.temperature(temperature: 0.2)
);
```

See more in the [documentation](https://docs.nobodywho.ooo/flutter/sampling/).

### Embeddings & RAG 

When you want your LLM to search through documents, understand semantic similarity, or build retrieval-augmented generation (RAG) systems, you'll need embeddings and cross-encoders.

Here's a complete example building a customer service assistant with access to company policies :

```dart
import 'package:nobodywho_dart/nobodywho_dart.dart' as nobodywho;

Future<void> main() async {
  // Initialize the cross-encoder for document ranking
  final crossencoder = await nobodywho.CrossEncoder.fromPath(modelPath: './reranker-model.gguf');

  // Your knowledge base
  final knowledge = [
    "Our company offers a 30-day return policy for all products",
    "Free shipping is available on orders over \$50",
    "Customer support is available via email and phone",
    "We accept credit cards, PayPal, and bank transfers",
    "Order tracking is available through your account dashboard"
  ];

  // Create a tool that searches the knowledge base
  final searchKnowledgeTool = nobodywho.Tool(
    function: ({required String query}) async {
      // Rank all documents by relevance to the query
      final ranked = await crossencoder.rankAndSort(query: query, documents: knowledge);

      // Return top 3 most relevant documents
      final topDocs = ranked.take(3).map((e) => e.$1).toList();
      return topDocs.join("\n");
    },
    name: "search_knowledge",
    description: "Search the knowledge base for relevant information"
  );

  // Create a chat with access to the knowledge base
  final chat = await nobodywho.Chat.fromPath(
    modelPath: './model.gguf',
    systemPrompt: "You are a customer service assistant. Use the search_knowledge tool to find relevant information from our policies before answering customer questions.",
    tools: [searchKnowledgeTool]
  );

  // The chat will automatically search the knowledge base when needed
  final response = await chat.ask("What is your return policy?").completed();
  print(response);
}
```

See more in the [documentation](https://docs.nobodywho.ooo/flutter/embeddings-and-rag/).
