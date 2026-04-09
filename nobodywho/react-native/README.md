# NobodyWho React Native

NobodyWho is a React Native library for running large language models locally and offline on iOS and Android.

Free to use in commercial projects under the EUPL-1.2 license — no API key required. Supports text, vision, embeddings, RAG & function calling.

- [Documentation](https://docs.nobodywho.ooo) — React Native & other frameworks documentation
- [Discord](https://discord.gg/qhaMc2qCYB) — Get help, share ideas, and connect with other developers
- [GitHub Issues](https://github.com/nobodywho-ooo/nobodywho/issues) — Report bugs
- [GitHub Discussions](https://github.com/nobodywho-ooo/nobodywho/discussions) — Ask questions and request features

## Quick Start

Install the library:

```
npm install react-native-nobodywho
```

## Supported Model Format

This library uses the **GGUF format** — a binary format optimized for fast loading and efficient LLM inference. A wide selection of GGUF models is available on [Hugging Face](https://huggingface.co/models).

**Compatibility notes:**
- Most GGUF models will work, but some may fail due to formatting issues.
- For mobile devices, models under 1 GB tend to run smoothly. As a general rule, the device should have at least twice the available RAM as the model file size. Note that available RAM differs from total RAM — iOS typically reserves around 1–2 GB for the kernel and system processes, while Android overhead varies by manufacturer: roughly 2 GB on stock Android (e.g. Pixel devices), and between 2–4 GB on Samsung, Xiaomi, and Oppo devices due to additional services.

**Minimum recommended specs:**

- iOS: iPhone 11 or newer with at least 4 GB of RAM.
- Android: Snapdragon 855 / Adreno 640 / 6 GB RAM or better.

## Chat

```typescript
import { Chat } from "react-native-nobodywho";

const chat = await Chat.fromPath({
  modelPath: "/path/to/model.gguf",
  systemPrompt: "You are a helpful assistant.",
});

// Stream tokens
for await (const token of chat.ask("Is water wet?")) {
  process.stdout.write(token);
}

// Or get the full response
const response = await chat.ask("Is water wet?").completed();
```

See the [Chat documentation](https://docs.nobodywho.ooo/react-native/chat/) for details.

## Tool Calling

Give your LLM the ability to interact with the outside world by defining tools:

```typescript
import { Chat, Tool } from "react-native-nobodywho";

const getWeather = new Tool({
  name: "get_weather",
  description: "Get the current weather for a city",
  parameters: {
    city: { type: "string", description: "The city name" },
  },
  call: ({ city }) => JSON.stringify({ temp: 22, condition: "sunny" }),
});

const chat = await Chat.fromPath({
  modelPath: "/path/to/model.gguf",
  tools: [getWeather],
});

const response = await chat.ask("What's the weather in Paris?").completed();
```

See the [Tool Calling documentation](https://docs.nobodywho.ooo/react-native/tool-calling/) for more.

---

## Sampling

The model outputs a probability distribution over possible tokens. A sampler determines how the next token is selected from that distribution. You can configure sampling to improve output quality or constrain outputs to a specific format (e.g. JSON):

```typescript
import { Chat, SamplerPresets } from "react-native-nobodywho";

const chat = await Chat.fromPath({
  modelPath: "/path/to/model.gguf",
  sampler: SamplerPresets.temperature(0.2), // Lower = more deterministic
});
```

See the [Sampling documentation](https://docs.nobodywho.ooo/react-native/sampling/) for more.

---

## Vision

Vision support lets you include images in your prompts, so the model can analyze and describe visual content alongside text.

To enable this, you need two model files:

- A **vision-language LLM** (usually has `VL` in the name)
- A matching **projection model** that converts images into tokens the LLM can process (usually has `mmproj` in the name)

Pass the projection model when loading your model, then use `Prompt` to compose prompts that mix text and images:

```typescript
import { Chat, Prompt } from "react-native-nobodywho";

const chat = await Chat.fromPath({
  modelPath: "/path/to/vision-model.gguf",
  imageModelPath: "/path/to/mmproj.gguf",
});

const prompt = new Prompt([
  Prompt.Text("What do you see in this image?"),
  Prompt.Image("/path/to/photo.png"),
]);

const response = await chat.ask(prompt).completed();
```

You can pass multiple images and interleave text between them. If the model performs poorly, try reordering the text and image parts — this can make a noticeable difference. If images consume too much context, increase `contextSize` or preprocess images with compression.

See the [Vision documentation](https://docs.nobodywho.ooo/react-native/vision/) for model recommendations and advanced tips.
