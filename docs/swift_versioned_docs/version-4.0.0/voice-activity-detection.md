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

```swift
import NobodyWho

let vad = try await VoiceActivityDetection.load(sampleRate: 16000, source: "hf://onnx-community/silero-vad")
let stt = try await SpeechToText.load(source: "hf://onnx-community/whisper-base")

while let chunk = readMic() { // however you're reading from the microphone
    if try vad.push(chunk: chunk) == .speechEnded {
        break
    }
}

let speech = vad.finish()
let transcription = try await stt.transcribePcm(samples: speech, sampleRate: 16000).completed()
print(transcription)
```

`VoiceActivityDetection` acts as a buffer: every `push` call tells you the current state
(`.speechStarted`, `.speechEnded`, `.speech`, or `.silence`), so you can decide when to stop
listening. Once you do, `finish()` gives you back the buffered audio for just the speech segment,
and resets internal state so it's ready for the next turn.

## Segmentation

Another use case is segmenting speech out of audio you already have. A good example is a long,
mostly-silent recording with a few short occurrences of speech you want to transcribe. That's what
the `segment` method is for:

```swift
import NobodyWho

let vad = try await VoiceActivityDetection.load(sampleRate: 16000, source: "hf://onnx-community/silero-vad")
let stt = try await SpeechToText.load(source: "hf://onnx-community/whisper-base")

let audio = readWavPcm(path: "recording.wav") // a full recording as [Int16] PCM samples

for speech in try vad.segment(samples: audio) {
    let transcription = try await stt.transcribePcm(samples: speech, sampleRate: 16000).completed()
    print(transcription)
}
```

## Configuring sensitivity

We try to provide reasonable defaults to capture most situations. However, especially in the case
of voice activity detection, manual tuning is often needed to reach better performance. For that,
we provide numerous params that you can tweak:

```swift
let vad = try await VoiceActivityDetection.load(
    sampleRate: 16000,
    // VAD is currently fixed to Silero ONNX, but you can change the source.
    source: "hf://onnx-community/silero-vad",
    // Determines the sensitivity to what counts as speech. Moving it up will make the VAD stricter.
    threshold: 0.5,
    // Determines the minimum duration classified as speech.
    minSpeechDurationMs: 250,
    // Determines the minimum duration classified as silence.
    minSilenceDurationMs: 250,
    // Determines how much audio to keep before the official .speechStarted, to avoid cutting off the start.
    prerollDurationMs: 500
)
```
