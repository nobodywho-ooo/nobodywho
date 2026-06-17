import os
import wave
import array

import nobodywho
import pytest


@pytest.fixture(scope="module")
def multimodal_model():
    model_path = os.environ.get("TEST_MULTIMODAL_MODEL")
    if not model_path:
        raise ValueError("TEST_MULTIMODAL_MODEL environment variable is not set")

    image_model_path = os.environ.get("TEST_MMPROJ_MODEL")
    if not image_model_path:
        raise ValueError("TEST_MMPROJ_MODEL environment variable is not set")

    return nobodywho.Model(model_path, projection_model_path=image_model_path)


@pytest.fixture
def multimodal_chat(multimodal_model):
    return nobodywho.Chat(
        multimodal_model,
        system_prompt="You are a helpful assistant.",
        template_variables={"enable_thinking": False},
        sampler=nobodywho.SamplerPresets.greedy(),
    )


def _read_bytes(rel_path: str) -> bytes:
    """Read a test fixture as raw bytes."""
    with open(os.path.join(os.path.dirname(__file__), rel_path), "rb") as f:
        return f.read()


def _read_wav_as_pcm(rel_path: str) -> tuple[list[int], int]:
    """Decode a WAV fixture into mono i16 PCM samples + sample rate.

    Mirrors what a realistic caller would do when they have audio in memory —
    e.g. from a microphone capture (already PCM) or after decoding an MP3
    download with `soundfile` / `librosa`. The `wave` module is stdlib-only,
    so this helper is self-contained.
    """
    with wave.open(os.path.join(os.path.dirname(__file__), rel_path), "rb") as wf:
        sample_rate = wf.getframerate()
        n_channels = wf.getnchannels()
        sample_width = wf.getsampwidth()
        n_frames = wf.getnframes()
        raw = wf.readframes(n_frames)

    assert sample_width == 2, (
        f"test WAV must be 16-bit (got {sample_width * 8}-bit)"
    )

    # Parse the interleaved i16 little-endian PCM.
    samples = array.array("h")
    samples.frombytes(raw)

    # Downmix to mono if needed.
    if n_channels > 1:
        mono = [
            sum(samples[i : i + n_channels]) // n_channels
            for i in range(0, len(samples), n_channels)
        ]
    else:
        mono = list(samples)

    return mono, sample_rate


# ---------- image: bytes is the primary path ----------


def test_image_description(multimodal_chat):
    prompt = nobodywho.Prompt(
        [
            nobodywho.Text(
                "What animal is in this image? Short answer. Focus on the species, not the age or the breed."
            ),
            nobodywho.Image(_read_bytes("img/penguin.png")),
        ]
    )

    response = multimodal_chat.ask(prompt).completed()

    assert isinstance(response, str)
    assert len(response) > 0
    assert "penguin" in response.lower()


def test_multiple_images(multimodal_chat):
    prompt = nobodywho.Prompt(
        [
            nobodywho.Image(_read_bytes("img/penguin.png")),
            nobodywho.Image(_read_bytes("img/dog.png")),
            nobodywho.Text(
                "What animals are in these images? Short answer. Focus on the species, not the age or the breed."
            ),
        ]
    )
    response = multimodal_chat.ask(prompt).completed()
    assert "penguin" in response.lower()
    assert "dog" in response.lower()


def test_multiple_images_interleaved(multimodal_chat):
    prompt = nobodywho.Prompt(
        [
            nobodywho.Text("What animal is in the first image?"),
            nobodywho.Image(_read_bytes("img/penguin.png")),
            nobodywho.Text("What animal is in the second image?"),
            nobodywho.Image(_read_bytes("img/dog.png")),
            nobodywho.Text(
                "Short answer. Focus on the species, not the age or the breed."
            ),
        ]
    )

    response = multimodal_chat.ask(prompt).completed()
    assert "penguin" in response.lower()
    assert "dog" in response.lower()


def test_image_recollection(multimodal_chat):
    prompt = nobodywho.Prompt(
        [
            nobodywho.Text(
                "What animal is in this image? Short answer. Focus on the species, not the age or the breed."
            ),
            nobodywho.Image(_read_bytes("img/dog.png")),
        ]
    )

    response = multimodal_chat.ask(prompt).completed()
    assert "dog" in response.lower()

    response2 = multimodal_chat.ask(
        "What is the color of the flowers in the background of the image? Short answer."
    ).completed()
    assert "orange" in response2.lower()


# ---------- audio: PCM is the in-memory primary path ----------


def test_audio_transcription_from_pcm(multimodal_chat):
    samples, sample_rate = _read_wav_as_pcm("audio/sound_16k.wav")
    prompt = nobodywho.Prompt(
        [
            nobodywho.Text("Please transcribe this audio."),
            nobodywho.Audio.from_pcm(samples, sample_rate),
        ]
    )
    response = multimodal_chat.ask(prompt).completed()
    assert "billy" in response.lower()


def test_audio_pcm_and_image_bytes(multimodal_chat):
    samples, sample_rate = _read_wav_as_pcm("audio/sound_16k.wav")
    prompt = nobodywho.Prompt(
        [
            nobodywho.Text("Please transcribe this audio and describe the image."),
            nobodywho.Audio.from_pcm(samples, sample_rate),
            nobodywho.Image(_read_bytes("img/dog.png")),
        ]
    )
    response = multimodal_chat.ask(prompt).completed()
    assert "billy" in response.lower() and (
        "dog" in response.lower() or "retriever" in response.lower()
    )


# ---------- legacy path-based API still works ----------


def test_image_and_audio_from_paths(multimodal_chat):
    """Smoke test that Image(path) and Audio(path) still work for callers who
    haven't migrated to the in-memory variants."""
    image_path = os.path.join(os.path.dirname(__file__), "img/penguin.png")
    audio_path = os.path.join(os.path.dirname(__file__), "audio/sound_16k.wav")

    prompt = nobodywho.Prompt(
        [
            nobodywho.Text(
                "Please transcribe this audio and identify the animal in the image."
            ),
            nobodywho.Audio(audio_path),
            nobodywho.Image(image_path),
        ]
    )
    response = multimodal_chat.ask(prompt).completed()
    assert "billy" in response.lower()
    assert "penguin" in response.lower()


