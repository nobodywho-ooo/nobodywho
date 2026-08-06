"""
Smoke test for the Vad (Silero VAD) binding.

Requires the Silero VAD ONNX model — set TEST_VAD_MODEL to a HuggingFace repo
(`hf://owner/repo`) or local directory path. Defaults to
"hf://onnx-community/silero-vad" so the model is downloaded automatically on
first run (cached after).

Uses the same shared test asset as test_stt.py (assets/sound_16k.wav), which
contains real speech ("Hey Ron. Hey Billy.").

The default threshold/min_speech_duration_ms combo is tuned for typical
mic-level speech and doesn't confirm SpeechStarted on this particular quiet
clip, so the test loosens both to match what the clip actually produces.
"""

import array
import os
import wave
import nobodywho
import pytest

MODEL = os.environ.get("TEST_VAD_MODEL", "hf://onnx-community/silero-vad")
AUDIO_WAV = os.environ.get(
    "TEST_AUDIO_FILE_WAV",
    os.path.join(
        os.path.dirname(__file__), "..", "..", "..", "assets", "sound_16k.wav"
    ),
)


def _read_wav_mono_i16(path):
    """Read a WAV file as mono i16 PCM samples, downmixing if needed."""
    with wave.open(path, "rb") as w:
        n_channels = w.getnchannels()
        sample_rate = w.getframerate()
        raw = w.readframes(w.getnframes())
    samples = array.array("h")
    samples.frombytes(raw)
    if n_channels > 1:
        samples = array.array(
            "h",
            (
                sum(samples[i : i + n_channels]) // n_channels
                for i in range(0, len(samples), n_channels)
            ),
        )
    return samples, sample_rate


@pytest.fixture(scope="module")
def audio():
    return _read_wav_mono_i16(AUDIO_WAV)


def test_push_detects_speech_and_finish_returns_audio(audio):
    samples, sample_rate = audio
    vad = nobodywho.Vad(
        source=MODEL,
        sample_rate=sample_rate,
        threshold=0.3,
        min_speech_duration_ms=90,
    )

    chunk_size = 800
    started = False
    ended = False
    for i in range(0, len(samples), chunk_size):
        event = vad.push(list(samples[i : i + chunk_size]))
        if event == nobodywho.VadEvent.SpeechStarted:
            started = True
        elif event == nobodywho.VadEvent.SpeechEnded:
            ended = True
            break

    assert started, "expected SpeechStarted to fire on real speech audio"
    assert ended, "expected SpeechEnded to fire once speech stops"

    captured = vad.finish()
    assert len(captured) > 0
    # Captured turn should be shorter than the full clip but non-trivial.
    assert len(captured) < len(samples)


def test_finish_is_empty_when_no_speech_confirmed():
    vad = nobodywho.Vad(source=MODEL, sample_rate=16000)
    silence = [0] * 512
    for _ in range(5):
        event = vad.push(silence)
        assert event is None
    assert vad.finish() == []


def test_predict_returns_one_probability_per_frame(audio):
    samples, sample_rate = audio
    vad = nobodywho.Vad(source=MODEL, sample_rate=sample_rate)
    probs = vad.predict(list(samples))
    assert len(probs) > 0
    assert all(0.0 <= p <= 1.0 for p in probs)


def test_segment_finds_speech_in_full_recording(audio):
    samples, sample_rate = audio
    vad = nobodywho.Vad(
        source=MODEL,
        sample_rate=sample_rate,
        threshold=0.3,
        min_speech_duration_ms=90,
    )
    segments = vad.segment(list(samples))
    assert len(segments) > 0
    for segment in segments:
        assert 0 < len(segment) < len(samples)
