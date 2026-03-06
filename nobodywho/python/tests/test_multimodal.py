import os

import nobodywho
import pytest


@pytest.fixture(scope="module")
def vision_model():
    model_path = os.environ.get("TEST_VISION_MODEL")
    if not model_path:
        raise ValueError("TEST_VISION_MODEL environment variable is not set")

    image_model_path = os.environ.get("TEST_MMPROJ_MODEL")
    if not image_model_path:
        raise ValueError("TEST_MMPROJ_MODEL environment variable is not set")

    return nobodywho.Model(model_path, image_model_path=image_model_path)


@pytest.fixture
def vision_chat(vision_model):
    return nobodywho.Chat(
        vision_model, system_prompt="You are a helpful assistant.", allow_thinking=False
    )


def test_image_description(vision_chat):
    """Test that the model can describe an image"""
    image_path = os.path.join(os.path.dirname(__file__), "img/penguin.png")
    prompt = nobodywho.Prompt(
        [
            nobodywho.Text(
                "What animal is in this image? Short answer. Focus on the species, not the age."
            ),
            nobodywho.Image(image_path),
        ]
    )

    response = vision_chat.ask(prompt).completed()

    assert isinstance(response, str)
    assert len(response) > 0
    assert "penguin" in response.lower()


def test_multiple_images(vision_chat):
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
                "What animals are in these images? Short answer. Focus on the species, not the age."
            ),
        ]
    )
    response = vision_chat.ask(prompt).completed()
    assert isinstance(response, str)
    assert len(response) > 0
    assert "penguin" in response.lower()
    assert "dog" in response.lower()


def test_multiple_images_interleaved(vision_chat):
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
            nobodywho.Text("Short answer. Focus on the species, not the age."),
        ]
    )

    response = vision_chat.ask(prompt).completed()
    assert isinstance(response, str)
    assert len(response) > 0
    assert "penguin" in response.lower()
    assert "dog" in response.lower()


def test_image_recollection(vision_chat):
    """Test that the model can recollect images"""
    image_path = os.path.join(os.path.dirname(__file__), "img/dog.png")
    prompt = nobodywho.Prompt(
        [
            nobodywho.Text(
                "What animal is in this image? Short answer. Focus on the species, not the age."
            ),
            nobodywho.Image(image_path),
        ]
    )

    response = vision_chat.ask(prompt).completed()
    assert isinstance(response, str)
    assert len(response) > 0
    assert "dog" in response.lower()

    response2 = vision_chat.ask(
        "What is the color of the flowers in the background of the image? Short answer."
    ).completed()
    assert isinstance(response2, str)
    assert len(response2) > 0
    assert "orange" in response2.lower()
