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
        multimodal_model,
        system_prompt="You are a helpful assistant.",
        template_variables={"enable_thinking": False},
        sampler=nobodywho.SamplerPresets.greedy(),
    )


def test_logging_can_be_disabled(capfd, caplog):
    model_path = os.environ.get("TEST_VISION_MODEL")
    projection_model_path = os.environ.get("TEST_MMPROJ_MODEL")
    if not model_path or not projection_model_path:
        raise ValueError("Multimodal model environment variables are not set")

    image_path = os.path.join(os.path.dirname(__file__), "img/dog.png")
    prompt = nobodywho.Prompt(
        [nobodywho.Text("Describe this image."), nobodywho.Image(image_path)]
    )

    def load_model_and_tokenize_image():
        model = nobodywho.Model(
            model_path,
            projection_model_path=projection_model_path,
        )
        chat = nobodywho.Chat(model)
        chat.tokenize(prompt)
        return model, chat

    def native_logs_were_captured():
        return any(
            record.name == "llama-cpp-2" or record.name.startswith("nobodywho")
            for record in caplog.records
        )

    with caplog.at_level(1):
        nobodywho.set_logging(enabled=True)
        capfd.readouterr()
        caplog.clear()
        enabled_model, enabled_chat = load_model_and_tokenize_image()
        enabled_stderr = capfd.readouterr().err
        enabled_logs_captured = native_logs_were_captured()

        del enabled_chat, enabled_model
        capfd.readouterr()
        caplog.clear()

        try:
            nobodywho.set_logging(enabled=False)
            disabled_model, disabled_chat = load_model_and_tokenize_image()
            disabled_stderr = capfd.readouterr().err
            disabled_logs_captured = native_logs_were_captured()
            del disabled_chat, disabled_model
        finally:
            nobodywho.set_logging(enabled=True)

    assert enabled_logs_captured
    assert "add_media:" in enabled_stderr
    assert not disabled_logs_captured
    assert "add_media:" not in disabled_stderr


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
    audio_path = os.path.join(
        os.path.dirname(__file__), "..", "..", "..", "assets", "sound_16k.wav"
    )
    prompt = nobodywho.Prompt(
        [
            nobodywho.Text("Please transcribe this audio."),
            nobodywho.Audio(audio_path),
        ]
    )
    response = multimodal_chat.ask(prompt).completed()
    assert "billy" in response.lower()


def test_audio_transcription_and_image_ingestion(multimodal_chat):
    """Test that the model can transcribe audio"""
    audio_path = os.path.join(
        os.path.dirname(__file__), "..", "..", "..", "assets", "sound_16k.wav"
    )
    image_path = os.path.join(os.path.dirname(__file__), "img/dog.png")
    prompt = nobodywho.Prompt(
        [
            nobodywho.Text("Please transcribe this audio and describe the image."),
            nobodywho.Audio(audio_path),
            nobodywho.Image(image_path),
        ]
    )
    response = multimodal_chat.ask(prompt).completed()
    assert "hey" in response.lower() and (
        "dog" in response.lower() or "retriever" in response.lower()
    )


def test_complete_with_content_parts(multimodal_chat):
    """Content parts interleave text and media in a single user message."""
    response = multimodal_chat.complete(
        [
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": "What animal is in this image?"},
                    {
                        "type": "image",
                        "path": os.path.join(os.path.dirname(__file__), "img/dog.png"),
                    },
                    {"type": "text", "text": "Answer in one word."},
                ],
            }
        ]
    ).completed()
    assert "dog" in response.lower()

    # The parts survive the round trip out of the history.
    content = multimodal_chat.get_chat_history()[0]["content"]
    assert [part["type"] for part in content] == ["text", "image", "text"]
    assert content[1]["path"] == os.path.join(os.path.dirname(__file__), "img/dog.png")


def test_complete_rejects_media_in_system_message(multimodal_chat):
    with pytest.raises(ValueError):
        multimodal_chat.complete(
            [
                {
                    "role": "system",
                    "content": [
                        {
                            "type": "image",
                            "path": os.path.join(
                                os.path.dirname(__file__), "img/dog.png"
                            ),
                        }
                    ],
                },
                {"role": "user", "content": "Hello"},
            ]
        )
