import os
import pathlib
import pytest
import nobodywho

AUDIO_FILE = pathlib.Path(__file__).parent / "sound.mp3"


@pytest.fixture(scope="module")
def whisper_model():
    model_path = os.environ.get("TEST_WHISPER_MODEL")
    if not model_path:
        raise ValueError("TEST_WHISPER_MODEL environment variable is not set")
    return nobodywho.SpeechToText(model_path, language="en")


def test_speech_to_text_completed(whisper_model):
    transcript = whisper_model.transcribe(AUDIO_FILE).completed()
    assert "Ron" in transcript or "Billy" in transcript, (
        f"Expected 'Ron' or 'Billy' in transcript, got: {transcript!r}"
    )
