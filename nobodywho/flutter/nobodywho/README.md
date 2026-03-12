# NobodyWho Flutter

NobodyWho is a Flutter library for running large language models locally and offline on iOS, Android, macOS, Linux, and Windows.

Free to use in commercial projects under the EUPL-1.2 license — no API key required.

- [Documentation](https://docs.nobodywho.ooo) — Flutter & other frameworks documentation
- [Discord](https://discord.gg/qhaMc2qCYB) — Get help, share ideas, and connect with other developers
- [GitHub Issues](https://github.com/nobodywho-ooo/nobodywho/issues) — Report bugs
- [GitHub Discussions](https://github.com/nobodywho-ooo/nobodywho/discussions) — Ask questions and request features
- [Starter example app](https://github.com/nobodywho-ooo/flutter-starter-example) — Test this library in 2 minutes

## Quick Start

Install the library:

```
flutter pub add nobodywho
```

In your `main.dart`, initialize the engine before launching your app:

```dart
import 'package:nobodywho/nobodywho.dart' as nobodywho;

void main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await nobodywho.NobodyWho.init();
  runApp(const MyApp());
}
```

## Supported Model Format

This library uses the **GGUF format** — a binary format optimized for fast loading and efficient LLM inference. A wide selection of GGUF models is available on [Hugging Face](https://huggingface.co/models).

**Compatibility notes:**
- Most GGUF models will work, but some may fail due to formatting issues.
- For mobile devices, models under 1 GB tend to run smoothly. As a general rule, the device should have at least twice the available RAM as the model file size. Note that available RAM differs from total RAM — iOS typically reserves around 1–2 GB for the kernel and system processes, while Android overhead varies by manufacturer: roughly 2 GB on stock Android (e.g. Pixel devices), and between 2–4 GB on Samsung, Xiaomi, and Oppo devices due to additional services.

**Minimum recommended specs:**

- iOS: iPhone 11 or newer with at least 4 GB of RAM. We tested a Qwen3 0.6B (332 MB) on an iPhone X (iOS 16) and while it ran, performance was too slow to be practical.
- Android: Snapdragon 855 / Adreno 640 / 6 GB RAM or better. The same Qwen3 0.6B model performed notably better on a OnePlus 7 Pro (Android 12) than on the iPhone X tested above.

### Model Integration

You can provide a model in two ways:

1. **On-demand download** — Download the model when needed using a library like [background_downloader](https://pub.dev/packages/background_downloader). This keeps your app size small but requires extra implementation for model management.

2. **Bundle in assets** — Include the model directly in your `assets` folder. Simpler to set up and great for development, but significantly increases app size — not recommended for production.

### Bundling a model (optional, but ideal for testing)

First, create an `assets` folder at the root of your Flutter project if one doesn't exist, then register it in `pubspec.yaml`:

```yaml
flutter:
  assets:
    - assets/
```

Download a compatible GGUF model — for example, [this one](https://huggingface.co/bartowski/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf) — rename it `model.gguf`, and place it in the `assets` folder.

To access assets at runtime, add `path_provider`:

```
flutter pub add path_provider
```

Then copy the model to the app's documents directory:

```dart
import 'dart:io';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';
import 'package:nobodywho/nobodywho.dart' as nobodywho;

final dir = await getApplicationDocumentsDirectory();
final model = File('${dir.path}/model.gguf');

if (!await model.exists()) {
  final data = await rootBundle.load('assets/model.gguf');
  await model.writeAsBytes(data.buffer.asUint8List(), flush: true);
}
```

Use `model.path` in place of `'./model.gguf'` in the examples below.

## Chat

```dart
final chat = await nobodywho.Chat.fromPath(modelPath: './model.gguf');
final msg = await chat.ask('Is water wet?').completed();
print(msg); // Yes, indeed, water is wet!
```

You can customize the chat with a system prompt, conversation history, and much more. See the [Chat documentation](https://docs.nobodywho.ooo/flutter/chat/) for details.

## Tool Calling

Give your LLM the ability to interact with the outside world by defining tools:

```dart
import 'dart:math' as math;
import 'package:nobodywho/nobodywho.dart' as nobodywho;

final circleAreaTool = nobodywho.Tool(
  name: "circle_area",
  description: "Calculates the area of a circle given its radius",
  function: ({ required double radius }) {
    final area = math.pi * radius * radius;
    return "Circle with radius $radius has area ${area.toStringAsFixed(2)}";
  }
);

final getWeatherTool = AiTool(
  name: "get_weather",
  description: "Get the weather of a city",
  function: ({required String city}) async {
    final weather = await fetchWeather(city);
    return weather;
  },
);

final chat = await nobodywho.Chat.fromPath(
  modelPath: './model.gguf',
  tools: [circleAreaTool, getWeatherTool],
);

final response = await chat.ask('What is the area of a circle with a radius of 2?').completed();
print(response);
```

See the [Tool Calling documentation](https://docs.nobodywho.ooo/flutter/tool-calling/) for more.

---

## Sampling

The model outputs a probability distribution over possible tokens. A sampler determines how the next token is selected from that distribution. You can configure sampling to improve output quality or constrain outputs to a specific format (e.g. JSON):

```dart
final chat = await nobodywho.Chat.fromPath(
  modelPath: "./model.gguf",
  sampler: nobodywho.SamplerPresets.temperature(temperature: 0.2), // Lower = more deterministic
);
```

See the [Sampling documentation](https://docs.nobodywho.ooo/flutter/sampling/) for more.

---

## Embeddings & RAG

For semantic search, document similarity, or retrieval-augmented generation (RAG), NobodyWho supports embeddings and cross-encoders.

Here's a complete example building a customer service assistant with access to a company knowledge base:

```dart
import 'package:nobodywho/nobodywho.dart' as nobodywho;

Future<void> main() async {
  // Initialize the cross-encoder for document ranking
  // Recommended model: https://huggingface.co/gpustack/bge-reranker-v2-m3-GGUF/resolve/main/bge-reranker-v2-m3-Q8_0.gguf
  final crossencoder = await nobodywho.CrossEncoder.fromPath(modelPath: './reranker-model.gguf');

  final knowledge = [
    "Our company offers a 30-day return policy for all products",
    "Free shipping is available on orders over \$50",
    "Customer support is available via email and phone",
    "We accept credit cards, PayPal, and bank transfers",
    "Order tracking is available through your account dashboard",
  ];

  final searchKnowledgeTool = nobodywho.Tool(
    name: "search_knowledge",
    description: "Search the knowledge base for relevant information",
    function: ({required String query}) async {
      final ranked = await crossencoder.rankAndSort(query: query, documents: knowledge);
      final topDocs = ranked.take(3).map((e) => e.$1).toList();
      return topDocs.join("\n");
    },
  );

  final chat = await nobodywho.Chat.fromPath(
    modelPath: './model.gguf',
    systemPrompt: "You are a customer service assistant. Use the search_knowledge tool to find relevant information from our policies before answering customer questions.",
    tools: [searchKnowledgeTool],
  );

  final response = await chat.ask("What is your return policy?").completed();
  print(response);
}
```

See the [Embeddings & RAG documentation](https://docs.nobodywho.ooo/flutter/embeddings-and-rag/) for more.