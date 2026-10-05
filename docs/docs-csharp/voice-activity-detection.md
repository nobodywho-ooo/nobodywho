---
title: Voice Activity Detection
description: Detect speech automatically in an audio stream and cut out silent segments.
sidebar_position: 7
---

Before running transcription on audio, it's important to know when to stop listening and start
transcribing (and answering). This can be done in a simple way (a silence timeout), but that can
often be quite clunky and feel slow. Voice activity detection uses a small model that understands
the shape of speech and can reliably tell speech and silence apart.

## Streaming

The main use case for VAD is streaming audio into the model chunk by chunk. This is useful, for
example, to detect when to stop listening to the microphone and start running
transcription/answer generation.

To do that, we provide a `Push` method:

```csharp
using NobodyWho;

using var vad = await VoiceActivityDetection.LoadAsync(sampleRate: 16000, source: "hf://onnx-community/silero-vad");
using var stt = await SpeechToText.LoadAsync("hf://onnx-community/whisper-base");

while (true)
{
    short[] chunk = ReadMic(); // however you're reading from the microphone
    if (vad.Push(chunk) == VoiceActivityDetectionEvent.SpeechEnded)
        break;
}

short[] speech = vad.Finish();
string transcription = await stt.TranscribePcm(speech, sampleRate: 16000).CompletedAsync();
Console.WriteLine(transcription);
```

`VoiceActivityDetection` acts as a buffer: every `Push()` call tells you the current state
(`SpeechStarted`, `SpeechEnded`, `Speech`, or `Silence`), so you can decide when to stop
listening. Pass only the newest chunk each time, not the whole recording so far. Once you stop,
`Finish()` gives you back the buffered audio for just the speech segment, and resets internal
state so it's ready for the next turn.

`Push()`, `Finish()` and `Segment()` are synchronous and run on the calling thread. `Push()` only
processes the chunk you give it, so it keeps up with live audio; for a long recording, run
`Segment()` with `Task.Run` if you need to keep a UI thread free.

## Segmentation

Another use case is segmenting speech out of audio you already have. A good example is a long,
mostly-silent recording with a few short occurrences of speech you want to transcribe. That's what
the `Segment()` method is for:

```csharp
using NobodyWho;

using var vad = await VoiceActivityDetection.LoadAsync(sampleRate: 16000, source: "hf://onnx-community/silero-vad");
using var stt = await SpeechToText.LoadAsync("hf://onnx-community/whisper-base");

short[] audio = ReadWavPcm("recording.wav"); // a full recording as 16-bit PCM samples

foreach (short[] speech in vad.Segment(audio))
{
    string transcription = await stt.TranscribePcm(speech, sampleRate: 16000).CompletedAsync();
    Console.WriteLine(transcription);
}
```

## Configuring sensitivity

We try to provide reasonable defaults to capture most situations. However, especially in the case
of voice activity detection, manual tuning is often needed to reach better performance. For that,
we provide numerous params that you can tweak:

```csharp
using var vad = await VoiceActivityDetection.LoadAsync(
    sampleRate: 16000,
    // VAD is currently fixed to Silero ONNX, but you can change the source.
    source: "hf://onnx-community/silero-vad",
    // Determines the sensitivity to what counts as speech. Moving it up will make the VAD stricter.
    threshold: 0.5f,
    // Determines the minimum duration classified as speech.
    minSpeechDurationMs: 250,
    // Determines the minimum duration classified as silence.
    minSilenceDurationMs: 250,
    // Determines how much audio to keep before the official SpeechStarted, to avoid cutting off the start.
    prerollDurationMs: 500,
    // Where the model runs: Auto (CUDA when available), Cpu or Cuda.
    device: InferenceDevice.Auto);
```
