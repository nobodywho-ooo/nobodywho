# NobodyWho React Native / Expo

NobodyWho is a React Native / Expo library for running large language models locally and offline on iOS and Android.

Free to use in commercial projects under the EUPL-1.2 license — no API key required. Supports text, vision, hearing, speech-to-text, text-to-speech, voice activity detection, embeddings, RAG & function calling.

- [Documentation](https://docs.nobodywho.ooo) — React Native & other frameworks documentation
- [RN starter example app](https://github.com/nobodywho-ooo/react-native-starter-example) / [Expo starter example app](https://github.com/nobodywho-ooo/expo-starter-example) — Test this library in 5 minutes
- [Discord](https://discord.gg/qhaMc2qCYB) — Get help, share ideas, and connect with other developers
- [GitHub Issues](https://github.com/nobodywho-ooo/nobodywho/issues) — Report bugs
- [GitHub Discussions](https://github.com/nobodywho-ooo/nobodywho/discussions) — Ask questions and request features

## How do I get started?

First, install `react-native-nobodywho`.
```bash
# React Native
npm install react-native-nobodywho
# Expo
npx expo install react-native-nobodywho
```

### React Native

No additional initialization step is required — the native module is loaded automatically when you first import from the package.

### Expo

NobodyWho ships native code, so it does **not** run in [Expo Go](https://docs.expo.dev/get-started/set-up-your-environment/). You need a [development build](https://docs.expo.dev/develop/development-builds/introduction/).

#### • Continuous Native Generation (CNG) projects

In a managed project, `ios/` and `android/` are not committed. For the **first build**, run the command for the platform you are targeting (`run:ios` and `run:android` are alternatives — pick your target). It runs prebuild automatically since the native folders don't exist yet, installs pods, and builds NobodyWho in:

```bash
npx expo run:ios
# or
npx expo run:android
```

After you **upgrade** NobodyWho (or any other native dependency), the native folders already exist, so `run:*` would build against stale native code. Regenerate them from scratch, then rebuild:

```bash
npx expo prebuild --clean   # regenerate ios/ and android/ from scratch
npx expo run:ios            # then rebuild (or run:android)
```

#### • Bare projects

Here `ios/` and `android/` are committed. Autolinking registers the module on your next native build — just rebuild with `expo run:*` (or your existing native build). Do not run `prebuild --clean` here unless you intend to regenerate the native folders, since it overwrites manual native edits.

You can also build with [EAS Build](https://docs.expo.dev/build/introduction/) instead of building locally. No config plugin is required.

## Supported Model Format

This library uses the **GGUF format** — a binary format optimized for fast loading and efficient LLM inference. A wide selection of GGUF models is available on [Hugging Face](https://huggingface.co/models).

**Compatibility notes:**
- Most GGUF models will work, but some may fail due to formatting issues.
- For mobile devices, models under 1 GB tend to run smoothly. As a general rule, the device should have at least twice the available RAM as the model file size. Note that available RAM differs from total RAM — iOS typically reserves around 1–2 GB for the kernel and system processes, while Android overhead varies by manufacturer: roughly 2 GB on stock Android (e.g. Pixel devices), and between 2–4 GB on Samsung, Xiaomi, and Oppo devices due to additional services.

**Minimum recommended specs:**

- iOS: iPhone 11 or newer with at least 4 GB of RAM.
- Android: Snapdragon 855 / Adreno 640 / 6 GB RAM or better.

## Model Loading

Models can be loaded from a local file path or downloaded automatically from HuggingFace:

```typescript
import { Model } from "react-native-nobodywho";

// Download from HuggingFace (cached automatically)
const model = await Model.load({
  modelPath: "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf",
});

// Or load from a local file
const model = await Model.load({ modelPath: "/path/to/model.gguf" });
```

Downloaded models are cached on disk and reused on subsequent loads.

## Chat

```typescript
import { Chat } from "react-native-nobodywho";

const chat = await Chat.fromPath({
  modelPath: "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf",
  systemPrompt: "You are a helpful assistant.",
});

// Stream tokens
for await (const token of chat.ask("Is water wet?")) {
  console.log(token);
}

// Or get the full response
const response = await chat.ask("Is water wet?").completed();
```

See the [Chat documentation](https://docs.nobodywho.ooo/react-native/chat/) for details.

## Tool Calling

Give your LLM the ability to interact with the outside world by defining tools:

```typescript
import { Chat, Tool } from "react-native-nobodywho";

function getWeatherForCity(city: string): string {
  return JSON.stringify({ temp: 22, condition: "sunny" });
}

const getWeather = new Tool({
  name: "get_weather",
  description: "Get the current weather for a city",
  parameters: [
    { name: "city", type: "string", description: "The city name" },
  ],
  call: getWeatherForCity,
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

## Vision & Hearing

Provide image and audio information to your LLM.

To enable this, you need two model files:

- A multimodal LLM, so the LLM can consume image-tokens or/and audio-tokens
- A matching projection model, which converts images to image-tokens or/and audio to audio-tokens (usually has `mmproj` in the name)

Pass the projection model when loading your model, then use `Prompt` to compose prompts that mix text and images:

```typescript
import { Chat, Prompt } from "react-native-nobodywho";

const chat = await Chat.fromPath({
  modelPath: "/path/to/vision-model.gguf",
  projectionModelPath: "/path/to/mmproj.gguf",
});

const response = await chat
  .ask(
    new Prompt([
      Prompt.Text("Tell me what you see in the image and what you hear in the audio."),
      Prompt.Image("/path/to/dog.png"),
      Prompt.Audio("/path/to/sound.mp3"),
    ]),
  )
  .completed();
```

You can pass multiple images/audio files and interleave text between them. If the model performs poorly, try reordering the text, audio and image parts — this can make a noticeable difference. If images consume too much context, increase `contextSize` or preprocess images with compression.

See the [Vision & Hearing documentation](https://docs.nobodywho.ooo/react-native/vision/) for model recommendations and advanced tips.

---

## Speech to Text

Transcribe spoken audio into text using Whisper models in ONNX format:

```typescript
import { SpeechToText } from "react-native-nobodywho";

const stt = await SpeechToText.load({
  source: "hf://onnx-community/whisper-base",
});

const text = await stt.transcribeFile("recording.mp3").completed();
console.log(text);
```

You can also transcribe raw PCM buffers with `transcribePcm`, and stream the transcription token by token.

See the [Speech to Text documentation](https://docs.nobodywho.ooo/react-native/speech-to-text/) for more.

---

## Text to Speech

Generate natural-sounding speech from text, ready to save as a WAV file or play back in your app:

```typescript
import { TextToSpeech } from "react-native-nobodywho";

const tts = await TextToSpeech.load({
  source: "hf://NobodyWho/Kokoro-82M", // Hugging Face repo or local folder.
  voice: "bf_emma", // Voice to use from the model.
  language: "en-gb", // Language code for the input text.
});

const wav = await tts.synthesize("Hello from NobodyWho!");
// wav is a Uint8Array containing WAV bytes.
```

NobodyWho supports the Kokoro, Pocket TTS, and Supertonic speech synthesis architectures.

See the [Text to Speech documentation](https://docs.nobodywho.ooo/react-native/text-to-speech/) for more.

---

## Voice Activity Detection

Detect speech automatically in an audio stream, so you know when to stop listening to the microphone and start transcribing:

```typescript
import {
  VoiceActivityDetection,
  VoiceActivityDetectionEvent,
} from "react-native-nobodywho";

const vad = await VoiceActivityDetection.load({
  sampleRate: 16000,
  source: "hf://onnx-community/silero-vad",
});

while (true) {
  const chunk = readMic();
  if (vad.push(chunk) === VoiceActivityDetectionEvent.SpeechEnded) break;
}

const speech = vad.finish(); // buffered speech, ready to pass to SpeechToText
```

You can also segment speech out of an existing recording with `segment()`.

See the [Voice Activity Detection documentation](https://docs.nobodywho.ooo/react-native/voice-activity-detection/) for more.
