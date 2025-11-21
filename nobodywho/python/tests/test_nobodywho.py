import os

import nobodywho
import pytest


@pytest.fixture
def model():
    model_path = os.environ.get("TEST_MODEL")
    return nobodywho.Model(model_path)


@pytest.fixture
def chat(model):
    return nobodywho.Chat(
        model, system_prompt="You are a helpful assistant", allow_thinking=False
    )


@pytest.mark.asyncio
async def test_async_streaming(chat):
    """Test async streaming from demo_async.py"""
    prompt = "What is the capital of Denmark?"
    token_stream = chat.send_message(prompt)

    tokens = []
    while token := await token_stream.next_token():
        tokens.append(token)

    response = "".join(tokens)
    assert len(response) > 0
    assert "copenhagen" in response.lower()


@pytest.mark.asyncio
async def test_async_collect(chat):
    """Test async complete from demo_async.py"""
    response_stream = chat.send_message("What is the capital of Denmark?")
    response = await response_stream.collect()

    assert len(response) > 0
    assert "copenhagen" in response.lower()


def test_blocking_collect(chat):
    response_stream = chat.send_message("What is the capital of Denmark?")
    response = response_stream.collect_blocking()
    assert "copenhagen" in response.lower()


@pytest.mark.asyncio
async def test_multiple_prompts(chat):
    """Test multiple sequential prompts like the demo loop"""
    prompts = ["Hello", "What is 2+2?", "Goodbye"]

    for prompt in prompts:
        response_stream = chat.send_message(prompt)
        response = await response_stream.collect()
        assert len(response) > 0


def test_sync_iterator(chat):
    response_stream = chat.send_message("What is the capital of Denmark?")
    response_str: str = ""
    for token in response_stream:
        response_str += token
        assert isinstance(token, str)
        assert len(token) > 0
    assert "copenhagen" in response_str.lower()
