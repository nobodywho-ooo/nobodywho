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

To do that, `push` audio chunks in and watch the returned event:

```gdscript
var vad = await NobodyWhoVoiceActivityDetection.create("hf://onnx-community/silero-vad", {
    "sample_rate": 16000,
})
var stt = await NobodyWhoSpeechToText.create("hf://onnx-community/whisper-base", {})

# however you're reading from the microphone — here, an AudioEffectCapture
# added to the "Record" bus in the Audio panel, which yields stereo float
# frames that we convert to mono i16 bytes.
var capture: AudioEffectCapture = AudioServer.get_bus_effect(AudioServer.get_bus_index("Record"), 0) as AudioEffectCapture

func mic_chunk() -> PackedByteArray:
    var frames: PackedVector2Array = capture.get_buffer(capture.get_frames_available())
    var chunk := PackedByteArray()
    chunk.resize(frames.size() * 2)
    for i in frames.size():
        chunk.encode_s16(i * 2, int(clamp((frames[i].x + frames[i].y) / 2.0, -1.0, 1.0) * 32767))
    return chunk

while true:
    var chunk := mic_chunk()
    if chunk.is_empty():
        await get_tree().process_frame
        continue
    var event: String = vad.push(chunk)
    if event == "speech_ended":
        break

var speech: PackedByteArray = vad.finish()
var transcription: String = await stt.transcribe_pcm(speech, 16000)
print(transcription)
```

`push` always returns the current state as a string — `"speech_started"`, `"speech_ended"`,
`"speech"`, or `"silence"` — so you can decide when to stop listening. Once you do, `finish()`
gives you back the buffered audio for just the speech segment, and resets internal state so it's
ready for the next turn.

Chunks are `PackedByteArray`s of mono i16 PCM samples (little-endian) — the same format
`transcribe_pcm` takes. The sample rate can be anything; pass it as `sample_rate` when creating
the VAD and it is resampled internally.

## Segmentation

Another use case is segmenting speech out of audio you already have. A good example is a long,
mostly-silent recording with a few short occurrences of speech you want to transcribe. That's
what the `segment` method is for:

```gdscript
var audio: PackedByteArray = read_wav_pcm("res://recording.wav") # i16 PCM bytes

for speech in vad.segment(audio):
    var transcription: String = await stt.transcribe_pcm(speech, 16000)
    print(transcription)
```

Unlike `push`, `segment` correctly finds every speech segment regardless of buffer size — use it
for offline processing instead of live streaming. Each returned segment comes with a short
pre-roll so the first word isn't clipped.

## Configuring sensitivity

We try to provide reasonable defaults to capture most situations. However, especially in the case
of voice activity detection, manual tuning is often needed to reach better performance. Pass
these keys in the config when creating the VAD:

```gdscript
var vad = await NobodyWhoVoiceActivityDetection.create("hf://onnx-community/silero-vad", {
    # Determines the sensitivity to what counts as speech.
    # Moving it up will make the VAD stricter.
    "threshold": 0.5,
    # Determines the minimum duration classified as speech.
    "min_speech_duration_ms": 250,
    # Determines the minimum duration classified as silence.
    "min_silence_duration_ms": 250,
    # Determines how much audio to keep before the official "speech_started",
    # to avoid cutting off the start.
    "preroll_duration_ms": 500,
})
```

VAD is currently fixed to the Silero ONNX model, but you can change the source (any repo or
local folder with `onnx/model.onnx` in the standard Silero layout).
