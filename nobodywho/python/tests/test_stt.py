"""
Smoke test for the STT (Whisper) binding.

Requires the Whisper ONNX model — set TEST_WHISPER_MODEL to a HuggingFace repo ID
or local directory path.  Defaults to "onnx-community/whisper-base" so the model is
downloaded automatically on first run (cached after).

The test audio contains the phrase "Hey Ron. Hey Billy."
"""

import array
import os
import wave
import nobodywho
import pytest

MODEL = os.environ.get("TEST_WHISPER_MODEL", "onnx-community/whisper-base")
AUDIO = os.environ.get(
    "TEST_AUDIO_FILE",
    os.path.join(os.path.dirname(__file__), "..", "..", "..", "assets", "sound.mp3"),
)
AUDIO_WAV = os.environ.get(
    "TEST_AUDIO_FILE_WAV",
    os.path.join(
        os.path.dirname(__file__), "..", "..", "..", "assets", "sound_16k.wav"
    ),
)


@pytest.fixture(scope="module")
def stt():
    return nobodywho.STT(MODEL)


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


def test_transcribe_file(stt):
    text = stt.transcribe_file(AUDIO).completed()
    assert "ron" in text.lower()
    assert "billy" in text.lower()


def test_transcribe_wav_file(stt):
    text = stt.transcribe_file(AUDIO_WAV).completed()
    assert "ron" in text.lower()
    assert "billy" in text.lower()


def test_transcribe_pcm(stt):
    samples, sample_rate = _read_wav_mono_i16(AUDIO_WAV)
    text = stt.transcribe_pcm(list(samples), sample_rate).completed()
    assert "ron" in text.lower()
    assert "billy" in text.lower()
