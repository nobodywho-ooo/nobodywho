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

To do that, we provide a `push` interface:

```typescript
import {
  VoiceActivityDetection,
  VoiceActivityDetectionEvent,
  SpeechToText,
} from "react-native-nobodywho";

const vad = await VoiceActivityDetection.load({
  sampleRate: 16000,
  source: "hf://onnx-community/silero-vad",
});
const stt = await SpeechToText.load({ source: "hf://onnx-community/whisper-base" });

while (true) {
  const chunk = readMic(); // however you're reading from the microphone
  if (vad.push(chunk) === VoiceActivityDetectionEvent.SpeechEnded) break;
}

const speech = vad.finish();
const transcription = await stt.transcribePcm(speech, 16000).completed();
console.log(transcription);
```

`VoiceActivityDetection` acts as a buffer: every `push()` call tells you the current state
(`SpeechStarted`, `SpeechEnded`, `Speech`, or `Silence`), so you can decide when to stop listening.
Once you do, `finish()` gives you back the buffered audio for just the speech segment, and resets
internal state so it's ready for the next turn.

## Segmentation

Another use case is segmenting speech out of audio you already have. A good example is a long,
mostly-silent recording with a few short occurrences of speech you want to transcribe. That's what
the `segment()` method is for:

```typescript
import { VoiceActivityDetection, SpeechToText } from "react-native-nobodywho";

const vad = await VoiceActivityDetection.load({
  sampleRate: 16000,
  source: "hf://onnx-community/silero-vad",
});
const stt = await SpeechToText.load({ source: "hf://onnx-community/whisper-base" });

const audio = readWavPcm("recording.wav"); // a full recording as i16 PCM samples

for (const speech of vad.segment(audio)) {
  const transcription = await stt.transcribePcm(speech, 16000).completed();
  console.log(transcription);
}
```

## Configuring sensitivity

We try to provide reasonable defaults to capture most situations. However, especially in the case
of voice activity detection, manual tuning is often needed to reach better performance. For that,
we provide numerous params that you can tweak:

```typescript
const vad = await VoiceActivityDetection.load({
  sampleRate: 16000,
  // VAD is currently fixed to Silero ONNX, but you can change the source.
  source: "hf://onnx-community/silero-vad",
  // Determines the sensitivity to what counts as speech. Moving it up will make the VAD stricter.
  threshold: 0.5,
  // Determines the minimum duration classified as speech.
  minSpeechDurationMs: 250,
  // Determines the minimum duration classified as silence.
  minSilenceDurationMs: 250,
  // Determines how much audio to keep before the official SpeechStarted, to avoid cutting off the start.
  prerollDurationMs: 500,
});
```
