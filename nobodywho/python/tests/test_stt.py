"""
Smoke test for the STT (Whisper) binding.

Requires the Whisper ONNX model — set TEST_WHISPER_MODEL to a HuggingFace repo ID
or local directory path.  Defaults to "onnx-community/whisper-base" so the model is
downloaded automatically on first run (cached after).

The test audio contains the phrase "Hey Ron. Hey Billy."
"""

import os
import nobodywho
import pytest

MODEL = os.environ.get("TEST_WHISPER_MODEL", "onnx-community/whisper-base")
AUDIO = os.environ.get(
    "TEST_AUDIO_FILE",
    os.path.join(os.path.dirname(__file__), "..", "..", "..", "assets", "sound.mp3"),
)


@pytest.fixture(scope="module")
def stt():
    return nobodywho.STT(MODEL)


def test_transcribe_file(stt):
    text = stt.transcribe_file(AUDIO).completed()
    assert "ron" in text.lower()
    assert "billy" in text.lower()
