import os

import nobodywho
import pytest

import logging

logging.addLevelName(5, "TRACE")


@pytest.fixture(scope="module")
def model():
    model_path = os.environ.get("TEST_MODEL")
    if not model_path:
        raise ValueError("TEST_MODEL environment variable is not set")

    return nobodywho.Model(model_path)


@pytest.fixture
def chat(model):
    return nobodywho.Chat(
        model, system_prompt="You are a helpful assistant", allow_thinking=False
    )


@nobodywho.tool(description="Applies the sparklify effect to a given piece of text.")
def sparklify(text: str) -> str:
    return f"✨{text.upper()}✨"


def test_tool_construction():
    assert sparklify is not None
    assert isinstance(sparklify, nobodywho.Tool)
    assert sparklify("foobar") == "✨FOOBAR✨"


def test_tool_calling(model):
    chat = nobodywho.Chat(model, tools=[sparklify])
    response: str = chat.ask("Please sparklify this word: 'julemand'").completed()
    assert "✨JULEMAND✨" in response


@nobodywho.tool(
    description="Boop foob",
    params={
        "reflarb": "the clump factor for the flopar",
        "unfloop": "activate the rotational velocidensity collider",
    },
)
def reflarbicator(reflarb: int, unfloop: bool) -> str:
    return "hahaha"


def test_tool_bad_parameters():
    with pytest.raises(TypeError):

        @nobodywho.tool(description="foobar", params={"b": "uh-oh"})
        def i_fucked_up(a: int) -> str:
            return "fuck"

@nobodywho.tool(
    description="Applies the sparklify effect to a given piece of text."
)
async def async_sparklify(text: str) -> str:
    return f"✨{text.upper()}✨"


@pytest.mark.asyncio
async def test_async_tool_construction():
    assert async_sparklify is not None
    assert isinstance(async_sparklify, nobodywho.Tool)
    assert await async_sparklify("foobar") == "✨FOOBAR✨"


def test_async_tool_calling(model):
    chat = nobodywho.Chat(model, tools=[async_sparklify])
    response: str = chat.ask("Please sparklify this word: 'julemand'").completed()
    assert "✨JULEMAND✨" in response


def test_async_tool_bad_parameters():
    with pytest.raises(TypeError):

        @nobodywho.tool(description="foobar", params={"b": "uh-oh"})
        async def i_fucked_up(a: int) -> str:
            return "fuck"


@nobodywho.tool(
    description="Gets the weather for a given location",
    params={"location": "The city or location to get weather for"}
)
def get_weather(location: str) -> str:
    return f"The weather in {location} is sunny and 21°C"


def test_set_tools(model):
    """Test that set_tools changes available tools and persists after reset_history"""
    chat = nobodywho.Chat(
        model, tools=[sparklify], allow_thinking=False
    )

    # Use initial tool
    resp1 = chat.ask("Please sparklify this word: 'julemand'").completed()
    assert isinstance(resp1, str)
    assert "✨JULEMAND✨" in resp1

    # Change tools
    chat.set_tools([get_weather])

    # Clear history but keep new tools
    chat.reset_history()

    # Try to use new tool - should work
    resp2 = chat.ask("What's the weather in Copenhagen?").completed()
    assert isinstance(resp2, str)
    assert "Copenhagen" in resp2 or "sunny" in resp2.lower()
