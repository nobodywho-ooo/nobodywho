# NobodyWho

**Run LLMs locally and efficiently on any device**

NobodyWho is a lightweight, open-source inference engine that makes it simple to run open-weights language models directly inside your Python applications. No API keys, no cloud infrastructure, no complexity—just fast, easy local AI.

Free to use in commercial projects under the EUPL-1.2 license, no API key required. Supports text, vision, hearing, speech-to-text, text-to-speech, voice activity detection, embeddings, RAG & tool calling.

- [Documentation](https://docs.nobodywho.ooo/python/) — Python & other frameworks documentation
- [Discord](https://discord.gg/qhaMc2qCYB) — Get help, share ideas, and connect with other developers
- [GitHub Issues](https://github.com/nobodywho-ooo/nobodywho/issues) — Report bugs
- [GitHub Discussions](https://github.com/nobodywho-ooo/nobodywho/discussions) — Ask questions and request features

## Key Features

- **Run locally, offline, for free** - No API keys or cloud services required
- **Fast, simple tool calling** - Just pass normal Python functions
- **Reliable tool execution** - Automatically derives grammar from function signatures
- **Speech-to-text** - Transcribe spoken audio into text with Whisper models
- **Text-to-speech** - Generate natural-sounding speech from text
- **Voice activity detection** - Detect speech in an audio stream to know when to start and stop listening
- **Vision & embeddings** - Multimodal image and audio input, plus embeddings and reranking for semantic search and RAG
- **Infinite conversations** - Conversation-aware preemptive context shifting prevents mid-conversation crashes
- **GPU accelerated** - Vulkan-powered inference for maximum performance
- **Thousands of compatible models** - Works with any LLM in GGUF format
- **Powered by llama.cpp** - Built on the proven [llama.cpp](https://github.com/ggml-org/llama.cpp) engine

## Installation

```bash
pip install nobodywho
```

## Supported Model Format

NobodyWho uses the **GGUF format**, a binary format optimized for fast loading and efficient LLM inference. A wide selection of GGUF models is available on [Hugging Face](https://huggingface.co/models).

You can also download a model without any extra dependencies by passing `huggingface:owner/repo/filename.gguf` where you'd normally pass the model path:

```python
from nobodywho import Chat

chat = Chat("huggingface:NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf")
```

## Chat

Every interaction with your LLM starts by instantiating a `Chat` object. Call `.ask()` to send a message, and `.completed()` to block until the whole response is ready:

```python
from nobodywho import Chat

chat = Chat("./model.gguf", system_prompt="You are a helpful assistant.")
response = chat.ask("Is water wet?").completed()
print(response)  # Yes, indeed, water is wet!
```

Your messages and the model's responses are remembered inside the `Chat` object, so follow-up questions keep their context. You can pass `"auto"` as the model path to pick a chat model based on available memory.

See the [Chat documentation](https://docs.nobodywho.ooo/python/chat/) for details.

## Tool Calling

Give your LLM the ability to interact with the outside world by turning any Python function into a tool with the `@tool` decorator. NobodyWho inspects the function signature to derive the parameters and configures the sampler for you:

```python
import math
from nobodywho import Chat, tool

@tool(description="Calculates the area of a circle given its radius")
def circle_area(radius: float) -> str:
    area = math.pi * radius ** 2
    return f"Circle with radius {radius} has area {area:.2f}"

chat = Chat("./model.gguf", tools=[circle_area])
response = chat.ask("What is the area of a circle with a radius of 2?").completed()
print(response)
```

See the [Tool Calling documentation](https://docs.nobodywho.ooo/python/tool-calling/) for more.

## Sampling

A sampler decides how the next token is picked from the model's probability distribution. Use a preset to tune creativity, or constrain the output to a specific format such as JSON:

```python
from nobodywho import Chat, SamplerPresets

# Lower temperature = more deterministic output
chat = Chat("./model.gguf", sampler=SamplerPresets.temperature(0.2))
```

You can also force the output to match a JSON schema, a regex, or a custom grammar:

```python
import json
from nobodywho import Chat, SamplerPresets

chat = Chat("./model.gguf", sampler=SamplerPresets.constrain_with_json_schema({
    "type": "object",
    "properties": {
        "name": {"type": "string", "maxLength": 50},
        "age":  {"type": "integer"},
    },
    "required": ["name", "age"],
    "additionalProperties": False,
}))
person = json.loads(chat.ask("Give me a person as JSON with name and age.").completed())
```

See the [Sampling documentation](https://docs.nobodywho.ooo/python/sampling/) for more.

## Embeddings & RAG

For semantic search, document similarity, or retrieval-augmented generation (RAG), NobodyWho supports embeddings and cross-encoders.

Turn text into vectors with an `Encoder` and compare them with `cosine_similarity`:

```python
from nobodywho import Encoder, cosine_similarity

encoder = Encoder("./embedding-model.gguf")
query = encoder.encode("How do I reset my password?")
doc = encoder.encode("You can reset your password in the account settings")
print(cosine_similarity(query, doc))
```

For more accurate ranking, use a `CrossEncoder` to build a knowledge-base search tool:

```python
from nobodywho import Chat, CrossEncoder, tool

crossencoder = CrossEncoder("./reranker-model.gguf")
knowledge = [
    "Our company offers a 30-day return policy for all products",
    "Free shipping is available on orders over $50",
    "Customer support is available via email and phone",
]

@tool(description="Search the knowledge base for relevant information")
def search_knowledge(query: str) -> str:
    ranked = crossencoder.rank_and_sort(query, knowledge)
    return "\n".join(doc for doc, score in ranked[:3])

chat = Chat(
    "./model.gguf",
    system_prompt="Use the search_knowledge tool to answer customer questions.",
    tools=[search_knowledge],
)
print(chat.ask("What is your return policy?").completed())
```

See the [Embeddings & RAG documentation](https://docs.nobodywho.ooo/python/embeddings-and-rag/) for more.

## Vision and Hearing

Include images and audio in your prompts, so the model can see and hear content alongside text. You need a multimodal LLM plus a matching projection model (usually named `mmproj`), which have to be trained together.

```python
from nobodywho import Model, Chat, Prompt, Text, Image, Audio

model = Model("./multimodal-model.gguf", projection_model_path="./mmproj.gguf")
chat = Chat(model, system_prompt="You are a helpful assistant that can hear and see!")

prompt = Prompt([
    Text("Tell me what you see in the image and what you hear in the audio."),
    Image("./dog.png"),
    Audio("./sound.mp3"),
])
print(chat.ask(prompt).completed())
```

See the [Multimodal documentation](https://docs.nobodywho.ooo/python/vision/) for model recommendations and advanced tips.

## Speech to Text

Transcribe spoken audio into text using Whisper models in ONNX format:

```python
from nobodywho import SpeechToText

stt = SpeechToText(source="hf://onnx-community/whisper-base")
text = stt.transcribe_file("recording.mp3").completed()
print(text)
```

You can also transcribe raw mono i16 PCM buffers with `transcribe_pcm`, and stream the transcription token by token.

See the [Speech to Text documentation](https://docs.nobodywho.ooo/python/speech-to-text/) for more.

## Text to Speech

Generate natural-sounding speech from text, ready to save as a WAV file or play back in your app:

```python
from pathlib import Path
from nobodywho import TextToSpeech

tts = TextToSpeech(
    source="hf://NobodyWho/Kokoro-82M",  # Hugging Face repo or local folder.
    voice="bf_emma",                     # Voice to use from the model.
    language="en-gb",                    # Language code for the input text.
)

wav = tts.synthesize("Hello from NobodyWho!")
Path("out.wav").write_bytes(wav)
```

NobodyWho supports the Kokoro, Pocket TTS, and Supertonic speech synthesis architectures.

See the [Text to Speech documentation](https://docs.nobodywho.ooo/python/text-to-speech/) for more.

## Voice Activity Detection

Detect speech automatically in an audio stream, so you know when to stop listening to the microphone and start transcribing:

```python
from nobodywho import VoiceActivityDetection, VoiceActivityDetectionEvent, SpeechToText

vad = VoiceActivityDetection(source="hf://onnx-community/silero-vad", sample_rate=16000)
stt = SpeechToText(source="hf://onnx-community/whisper-base")

while chunk := read_mic():  # however you read from the microphone
    if vad.push(chunk) == VoiceActivityDetectionEvent.SpeechEnded:
        break

speech = vad.finish()  # buffered speech, ready to pass to SpeechToText
print(stt.transcribe_pcm(speech, sample_rate=16000).completed())
```

You can also segment speech out of an existing recording with `segment()`.

See the [Voice Activity Detection documentation](https://docs.nobodywho.ooo/python/voice-activity-detection/) for more.

## Streaming & Async API

To stream tokens as soon as they arrive, iterate over the response instead of calling `.completed()`:

```python
from nobodywho import Chat

chat = Chat("./model.gguf")
for token in chat.ask("How are you?"):
    print(token, end="", flush=True)
```

For non-blocking inference, swap `Chat` for `ChatAsync` — the API is identical, and you can `await` a full response or stream tokens with `async for`:

```python
import asyncio
from nobodywho import ChatAsync

async def main():
    chat = ChatAsync("./model.gguf")
    async for token in chat.ask("How are you?"):
        print(token, end="", flush=True)

asyncio.run(main())
```

The other model types also have async variants: `EncoderAsync`, `CrossEncoderAsync`, and `SpeechToTextAsync`.

See the [Streaming & Async documentation](https://docs.nobodywho.ooo/python/streaming-and-async-api/) for more.

## Documentation

Full documentation available at: https://docs.nobodywho.ooo/python/

## License

EUPL-1.2 - Free for commercial and proprietary use. Modified versions of the library itself must remain open source.
