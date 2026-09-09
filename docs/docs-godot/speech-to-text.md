---
title: Speech to Text
description: Transcribe spoken audio to text with NobodyWho in Godot.
sidebar_position: 5
---

To transcribe audio into text, NobodyWho provides an integration with the Whisper models in ONNX
format.

```gdscript
var stt = await NobodyWhoSpeechToText.create("hf://onnx-community/whisper-base", {})

var text: String = await stt.transcribe_file("res://recording.mp3")
print(text)
```

If the audio is not coming from a file, but instead directly from a buffer, `transcribe_pcm` is
available:

```gdscript
var text: String = await stt.transcribe_pcm(pcm_bytes, 16000)
```

In order to make this work, the buffer needs to be mono i16 PCM samples, as a `PackedByteArray`
of interleaved little-endian bytes. The sample rate can be anything — NobodyWho resamples
internally to what Whisper expects. Godot's `AudioEffectCapture` yields stereo float frames, so
feed it through a small conversion first (see
[Voice Activity Detection](./voice-activity-detection) for a ready-made example).

As with chat, streaming is available, so the transcription can be consumed token by token:

```gdscript
var stream = stt.transcribe_file_stream("res://recording.mp3")
while true:
    var piece = await stream.next_token()
    if piece == null:
        break
    print(piece)
```

`transcribe_pcm_stream` works the same for raw buffers. Both also offer `completed()` if you
want the whole transcript in one `await`.

## Supported models

NobodyWho only supports Whisper models in **ONNX** format. The source is a Hugging Face repo
(`hf://owner/repo`) or a local directory containing such a model, e.g.
`hf://onnx-community/whisper-base`. Browse the
[Whisper ONNX models on Hugging Face](https://huggingface.co/models?library=onnx&search=whisper)
to pick a size that fits your accuracy and speed needs.

You can also pick a `quantization` variant of the model to download and load. Lower-precision
variants are smaller and faster, but can lose some transcription accuracy. Supported values are
`default`, `fp16`, `int8`, `uint8`, `bnb4`, `q4`, `q4f16`, and `quantized`. Defaults to `q4`,
falling back to `default` when the variant isn't in the repo.

```gdscript
var stt = await NobodyWhoSpeechToText.create("hf://onnx-community/whisper-base", {
    "quantization": "q4",
})
```

## Improving performance

By default, Whisper auto-detects the spoken language, which costs a bit of extra processing. If
you already know the language, pass its ISO 639-1 code as `language` to skip detection and
improve performance:

```gdscript
var stt = await NobodyWhoSpeechToText.create("hf://onnx-community/whisper-base", {
    "language": "en",
})
```

For more performance, the `"device"` config key accepts `"cpu"`, `"cuda"`, or `"auto"` (default).

If you're transcribing microphone input, consider pairing this with
[Voice Activity Detection](./voice-activity-detection) so you only transcribe actual speech
instead of a fixed-length buffer.
