import os

import nobodywho
import pytest


@pytest.fixture(scope="module")
def multimodal_model():
    model_path = os.environ.get("TEST_VISION_MODEL")
    if not model_path:
        raise ValueError("TEST_VISION_MODEL environment variable is not set")

    image_model_path = os.environ.get("TEST_MMPROJ_MODEL")
    if not image_model_path:
        raise ValueError("TEST_MMPROJ_MODEL environment variable is not set")

    return nobodywho.Model(model_path, projection_model_path=image_model_path)


@pytest.fixture
def multimodal_chat(multimodal_model):
    return nobodywho.Chat(
        multimodal_model, system_prompt="You are a helpful assistant.", template_variables={"enable_thinking": False}
    )


def test_image_description(multimodal_chat):
    """Test that the model can describe an image"""
    image_path = os.path.join(os.path.dirname(__file__), "img/penguin.png")
    prompt = nobodywho.Prompt(
        [
            nobodywho.Text(
                "What animal is in this image? Short answer. Focus on the species, not the age or the breed."
            ),
            nobodywho.Image(image_path),
        ]
    )

    response = multimodal_chat.ask(prompt).completed()

    assert isinstance(response, str)
    assert len(response) > 0
    assert "penguin" in response.lower()


def test_multiple_images(multimodal_chat):
    """Test that the model can describe multiple images"""
    image_paths = [
        os.path.join(os.path.dirname(__file__), "img/penguin.png"),
        os.path.join(os.path.dirname(__file__), "img/dog.png"),
    ]
    prompt = nobodywho.Prompt(
        [
            nobodywho.Image(image_paths[0]),
            nobodywho.Image(image_paths[1]),
            nobodywho.Text(
                "What animals are in these images? Short answer. Focus on the species, not the age or the breed."
            ),
        ]
    )
    response = multimodal_chat.ask(prompt).completed()
    assert isinstance(response, str)
    assert len(response) > 0
    assert "penguin" in response.lower()
    assert "dog" in response.lower()


def test_multiple_images_interleaved(multimodal_chat):
    """Test that the model can describe multiple images interleaved with text"""
    image_paths = [
        os.path.join(os.path.dirname(__file__), "img/penguin.png"),
        os.path.join(os.path.dirname(__file__), "img/dog.png"),
    ]
    prompt = nobodywho.Prompt(
        [
            nobodywho.Text("What animal is in the first image?"),
            nobodywho.Image(image_paths[0]),
            nobodywho.Text("What animal is in the second image?"),
            nobodywho.Image(image_paths[1]),
            nobodywho.Text(
                "Short answer. Focus on the species, not the age or the breed."
            ),
        ]
    )

    response = multimodal_chat.ask(prompt).completed()
    assert isinstance(response, str)
    assert len(response) > 0
    assert "penguin" in response.lower()
    assert "dog" in response.lower()


def test_image_recollection(multimodal_chat):
    """Test that the model can recollect images"""
    image_path = os.path.join(os.path.dirname(__file__), "img/dog.png")
    prompt = nobodywho.Prompt(
        [
            nobodywho.Text(
                "What animal is in this image? Short answer. Focus on the species, not the age or the breed."
            ),
            nobodywho.Image(image_path),
        ]
    )

    response = multimodal_chat.ask(prompt).completed()
    assert isinstance(response, str)
    assert len(response) > 0
    assert "dog" in response.lower()

    response2 = multimodal_chat.ask(
        "What is the color of the flowers in the background of the image? Short answer."
    ).completed()
    assert isinstance(response2, str)
    assert len(response2) > 0
    assert "orange" in response2.lower()



def test_audio_transcription(multimodal_chat):
    """Test that the model can transcribe audio"""
    audio_path = os.path.join(os.path.dirname(__file__), "audio/sound_16k.wav")
    prompt = nobodywho.Prompt([
        nobodywho.Text("Please transcribe this audio."),
        nobodywho.Audio(audio_path),
    ])
    response = multimodal_chat.ask(prompt).completed()
    assert "billy" in response.lower()


def test_audio_transcription_and_image_ingestion(multimodal_chat):
    """Test that the model can transcribe audio"""
    audio_path = os.path.join(os.path.dirname(__file__), "audio/sound_16k.wav")
    image_path = os.path.join(os.path.dirname(__file__), "img/dog.png")
    prompt = nobodywho.Prompt([
        nobodywho.Text("Please transcribe this audio and describe the image."),
        nobodywho.Audio(audio_path),
        nobodywho.Image(image_path),
    ])
    response = multimodal_chat.ask(prompt).completed()
    assert "billy" in response.lower() and "dog" in response.lower()


