---
title: Voice Activity Detection
description: Detect speech automatically in an audio stream and cut out silent segments.
sidebar_position: 10
---

Before running transcription on audio, it's important to know when to stop listening and start
transcribing (and answering). This can be done in a simple way (a silence timeout), but that can
often be quite clunky and feel slow. Voice activity detection uses a small model that understands
the shape of speech and can reliably tell speech and silence apart, through the
`NobodyWhoVoiceActivityDetection` node.

## Streaming

Add a `NobodyWhoVoiceActivityDetection` node to your scene, then push audio chunks into it as they
arrive from wherever you're capturing them (e.g. an `AudioEffectCapture`):

```gdscript
extends Node

@onready var vad: NobodyWhoVoiceActivityDetection = $NobodyWhoVoiceActivityDetection
@onready var stt: NobodyWhoSpeechToText = $NobodyWhoSpeechToText

func _ready():
    vad.speech_ended.connect(_on_speech_ended)

    vad.start_worker()
    await vad.worker_started
    stt.start_worker()
    await stt.worker_started

# Call this whenever you get a new chunk from your microphone capture setup.
func _on_mic_chunk(samples: PackedByteArray):
    vad.push(samples)

func _on_speech_ended():
    var speech = vad.finish()
    stt.transcribe_pcm(speech, 16000)
    var text = await stt.transcription_finished
    print(text)
```

`NobodyWhoVoiceActivityDetection` acts as a buffer: every `push()` call emits `speech_started` or
`speech_ended` when it crosses a confirmed boundary. Once `speech_ended` fires, `finish()` gives
you back the buffered audio for just the speech segment, and resets internal state so it's ready
for the next turn.

## Segmentation

Another use case is segmenting speech out of audio you already have. A good example is a long,
mostly-silent recording with a few short occurrences of speech you want to transcribe. That's what
the `segment()` method is for:

```gdscript
extends Node

@onready var vad: NobodyWhoVoiceActivityDetection = $NobodyWhoVoiceActivityDetection
@onready var stt: NobodyWhoSpeechToText = $NobodyWhoSpeechToText

func _ready():
    vad.start_worker()
    await vad.worker_started
    stt.start_worker()
    await stt.worker_started

    var audio: PackedByteArray = ... # a full recording, interleaved little-endian i16 PCM samples

    for speech in vad.segment(audio):
        stt.transcribe_pcm(speech, 16000)
        var text = await stt.transcription_finished
        print(text)
```

## Configuring sensitivity

We try to provide reasonable defaults to capture most situations. However, especially in the case
of voice activity detection, manual tuning is often needed to reach better performance. For that,
`NobodyWhoVoiceActivityDetection` exposes a few properties you can tweak, either in the editor or
from code — set them before calling `start_worker()`, since that's when they're read:

```gdscript
extends Node

@onready var vad: NobodyWhoVoiceActivityDetection = $NobodyWhoVoiceActivityDetection

func _ready():
    # VAD is currently fixed to Silero ONNX, but you can change the model_path.
    vad.model_path = "hf://onnx-community/silero-vad"
    # Determines the sensitivity to what counts as speech. Moving it up will make the VAD stricter.
    vad.threshold = 0.5
    # Determines the minimum duration classified as speech.
    vad.min_speech_duration_ms = 250
    # Determines the minimum duration classified as silence.
    vad.min_silence_duration_ms = 250
    # Determines how much audio to keep before the official speech_started signal, to avoid cutting off the start.
    vad.preroll_duration_ms = 500

    vad.start_worker()
    await vad.worker_started
```
