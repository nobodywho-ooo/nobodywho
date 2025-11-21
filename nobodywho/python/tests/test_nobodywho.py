import os

import nobodywho
import pytest


@pytest.fixture
def model():
    model_path = os.environ.get("TEST_MODEL")
    return nobodywho.Model(model_path)


@pytest.fixture
def chat(model):
    return nobodywho.Chat(model, system_prompt="You are a helpful assistant")


@pytest.mark.asyncio
async def test_async_streaming(chat):
    """Test async streaming from demo_async.py"""
    prompt = "What is 2+2? Answer in one word."
    token_stream = chat.say_stream(prompt)

    tokens = []
    while token := await token_stream.next_token_async():
        tokens.append(token)

    response = "".join(tokens)
    assert len(response) > 0
    assert "4" in response or "four" in response.lower()


@pytest.mark.asyncio
async def test_async_complete(chat):
    """Test async complete from demo_async.py"""
    prompt = "What is the capital of Denmark?"
    response = await chat.say_complete_async(prompt)

    assert len(response) > 0
    assert "copenhagen" in response.lower()


@pytest.mark.asyncio
async def test_multiple_prompts(chat):
    """Test multiple sequential prompts like the demo loop"""
    prompts = ["Hello", "What is 2+2?", "Goodbye"]

    for prompt in prompts:
        response = await chat.say_complete_async(prompt)
        assert len(response) > 0


def test_sync_iterator(chat):
    response_stream = chat.send_message("What is the capital of Copenhagen?")
    response_str: str = ""
    for token in response_stream:
        response_str += token
        assert isinstance(token, str)
        assert len(token) > 0
    assert "copenhagen" in response_str.lower()
