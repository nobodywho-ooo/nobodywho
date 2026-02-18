import os

import nobodywho
import pytest

import logging

logging.addLevelName(5, "TRACE")


def get_tool_calls(chat_history):
    """Extract all tool calls from chat history."""
    tool_calls = []
    for msg in chat_history:
        if "tool_calls" in msg:
            tool_calls.extend(msg["tool_calls"])
    return tool_calls


def get_tool_responses(chat_history):
    """Extract all tool responses from chat history."""
    return [msg for msg in chat_history if msg.get("role") == "tool"]

@nobodywho.tool(
    description="Boop foob",
    params={
        "reflarb": "the clump factor for the flopar",
        "unfloop": "activate the rotational velocidensity collider",
    },
)
def reflarbicator(reflarb: int, unfloop: bool) -> str:
    return "hahaha"


@nobodywho.tool(
    description="Gets the weather for a given location",
    params={"location": "The city or location to get weather for"}
)
def get_weather(location: str) -> str:
    return f"The weather in {location} is sunny and 21°C"


@nobodywho.tool(
    description="Applies the sparklify effect to a given piece of text."
)
async def async_sparklify(text: str) -> str:
    return f"✨{text.upper()}✨"



@nobodywho.tool(description="Applies the sparklify effect to a given piece of text.")
def sparklify(text: str) -> str:
    return f"✨{text.upper()}✨"


@nobodywho.tool(description="Returns the intersection of set1 and set2")
def set_intersection(set1 : set[int], set2 : set[int]) -> str:
    return str(set1.intersection(set2))

@nobodywho.tool(description="Returns the string in the tuple string_int_pair concatenated with itself a number of times equal to the integer in string_int_pair")
def multiply_strings(string_int_pair : tuple[str,int]) -> str:
    assert isinstance(string_int_pair, tuple)
    return string_int_pair[0] * string_int_pair[1]

@nobodywho.tool(description="Does vector addition on the list of vectors provided", params={"list_of_vectors" : "List of vectors to added. Each vector must of same length"})
def add_list_of_vectors(list_of_vectors : list[list[int]]) -> str:
    res = list_of_vectors[0].copy()
    for v in list_of_vectors[1:]:
        for i,coord in enumerate(v):
            res[i] += coord
    return str(res)

@nobodywho.tool(description="Returns the volume of a cube with the input dimenensions", params={"dimensions" : "A map containing the numeric values for the width, heigth and depth of a cube."})
def calculate_volume(dimensions: dict[str,float]) -> str:
      # dimensions: {"width": float, "height": float, "depth": float}
      return str(dimensions["width"] * dimensions["height"] * dimensions["depth"])


@pytest.fixture(scope="module")
def model():
    model_path = os.environ.get("TEST_MODEL")
    if not model_path:
        raise ValueError("TEST_MODEL environment variable is not set")

    return nobodywho.Model(model_path)

@pytest.fixture
def chat(model):
    if "qwen" in os.environ.get("TEST_MODEL", "").lower():
        return nobodywho.Chat(
            model, system_prompt="You are a helpful assistant", allow_thinking=False, tools=[set_intersection, multiply_strings, sparklify, reflarbicator, add_list_of_vectors, calculate_volume]
        )
    return nobodywho.Chat(
            model, system_prompt="You are a helpful assistant", allow_thinking=False, tools=[sparklify]
        )
    



def test_tool_construction():
    assert sparklify is not None
    assert isinstance(sparklify, nobodywho.Tool)
    assert sparklify("foobar") == "✨FOOBAR✨"


def test_tool_calling(chat):
    chat.ask("Please sparklify this word: 'julemand' and show me the result").completed()

    history = chat.get_chat_history()
    tool_calls = get_tool_calls(history)
    tool_responses = get_tool_responses(history)

    assert len(tool_calls) == 1
    assert tool_calls[0]["function"]["name"] == "sparklify"
    assert tool_calls[0]["function"]["arguments"]["text"] == "julemand"

    assert len(tool_responses) == 1
    assert tool_responses[0]["name"] == "sparklify"
    assert tool_responses[0]["content"] == "✨JULEMAND✨"




def test_tool_bad_parameters():
    with pytest.raises(TypeError):

        @nobodywho.tool(description="foobar", params={"b": "uh-oh"})
        def i_fucked_up(a: int) -> str:
            return "fuck"


@pytest.mark.asyncio
async def test_async_tool_construction():
    assert async_sparklify is not None
    assert isinstance(async_sparklify, nobodywho.Tool)
    assert await async_sparklify("foobar") == "✨FOOBAR✨"


def test_async_tool_calling(model):
    chat = nobodywho.Chat(model, tools=[async_sparklify])
    chat.ask("Please sparklify this word: 'julemand' and show me the result").completed()

    history = chat.get_chat_history()
    tool_calls = get_tool_calls(history)
    tool_responses = get_tool_responses(history)

    assert len(tool_calls) == 1
    assert tool_calls[0]["function"]["name"] == "async_sparklify"
    assert tool_calls[0]["function"]["arguments"]["text"] == "julemand"

    assert len(tool_responses) == 1
    assert tool_responses[0]["name"] == "async_sparklify"
    assert tool_responses[0]["content"] == "✨JULEMAND✨"


def test_async_tool_bad_parameters():
    with pytest.raises(TypeError):

        @nobodywho.tool(description="foobar", params={"b": "uh-oh"})
        async def i_fucked_up(a: int) -> str:
            return "fuck"



def test_set_tools(model):
    """Test that set_tools changes available tools and persists after reset_history"""
    chat = nobodywho.Chat(
            model, system_prompt="You are a helpful assistant", allow_thinking=False, tools=[sparklify]
        )
    # Use initial tool
    chat.ask("Please sparklify this word: 'julemand' and show me the result").completed()

    history = chat.get_chat_history()
    tool_calls = get_tool_calls(history)
    tool_responses = get_tool_responses(history)

    assert len(tool_calls) == 1
    assert tool_calls[0]["function"]["name"] == "sparklify"
    assert tool_calls[0]["function"]["arguments"]["text"] == "julemand"

    assert len(tool_responses) == 1
    assert tool_responses[0]["content"] == "✨JULEMAND✨"

    # Change tools
    chat.set_tools([get_weather])

    # Clear history but keep new tools
    chat.reset_history()

    # Try to use new tool - should work
    chat.ask("What's the weather in Copenhagen?").completed()

    history = chat.get_chat_history()
    tool_calls = get_tool_calls(history)
    tool_responses = get_tool_responses(history)

    assert len(tool_calls) == 1
    assert tool_calls[0]["function"]["name"] == "get_weather"
    assert tool_calls[0]["function"]["arguments"]["location"] == "Copenhagen"

    assert len(tool_responses) == 1
    assert tool_responses[0]["name"] == "get_weather"
    assert tool_responses[0]["content"] == "The weather in Copenhagen is sunny and 21°C"


def test_tool_calling_with_custom_sampler(model):
    chat = nobodywho.Chat(
        model,
        tools=[sparklify],
        sampler=nobodywho.SamplerBuilder()
            .top_k(64)
            .top_p(0.95, min_keep=2)
            .temperature(0.8)
            .dist(),
        allow_thinking=False,
    )

    chat.ask("Please sparklify this word: 'julemand' and show me the result").completed()

    history = chat.get_chat_history()
    tool_calls = get_tool_calls(history)
    tool_responses = get_tool_responses(history)

    assert len(tool_calls) == 1
    assert tool_calls[0]["function"]["name"] == "sparklify"
    assert tool_calls[0]["function"]["arguments"]["text"] == "julemand"

    assert len(tool_responses) == 1
    assert tool_responses[0]["name"] == "sparklify"
    assert tool_responses[0]["content"] == "✨JULEMAND✨"


def test_tool_with_sets(chat):
    if "qwen" not in os.environ.get("TEST_MODEL", "").lower():
        pytest.skip("Test only runs with Qwen models")

    chat.ask("Please use the provided tool to find the intersection between the sets {12,5,7,3,4} and {12,9,5,3}").completed()

    history = chat.get_chat_history()
    tool_calls = get_tool_calls(history)
    tool_responses = get_tool_responses(history)

    assert len(tool_calls) == 1
    assert tool_calls[0]["function"]["name"] == "set_intersection"
    assert set(tool_calls[0]["function"]["arguments"]["set1"]) == {12, 5, 7, 3, 4}
    assert set(tool_calls[0]["function"]["arguments"]["set2"]) == {12, 9, 5, 3}

    assert len(tool_responses) == 1
    assert tool_responses[0]["name"] == "set_intersection"
    # The response is a string representation of a set, check all expected elements are present
    response_content = tool_responses[0]["content"]
    assert "12" in response_content and "5" in response_content and "3" in response_content 

def test_tool_with_tuple(chat):
    if "qwen" not in os.environ.get("TEST_MODEL", "").lower():
        pytest.skip("Test only runs with Qwen models")

    chat.ask("Please use the provided tool to multiply the string BingBong by 3").completed()

    history = chat.get_chat_history()
    tool_calls = get_tool_calls(history)
    tool_responses = get_tool_responses(history)

    assert len(tool_calls) == 1
    assert tool_calls[0]["function"]["name"] == "multiply_strings"
    assert tool_calls[0]["function"]["arguments"]["string_int_pair"] == ["BingBong", 3]

    assert len(tool_responses) == 1
    assert tool_responses[0]["name"] == "multiply_strings"
    assert tool_responses[0]["content"] == "BingBongBingBongBingBong"

def test_tool_with_nested_list(chat):
    if "qwen" not in os.environ.get("TEST_MODEL", "").lower():
        pytest.skip("Test only runs with Qwen models")

    chat.ask("Please use the provided tool to add the vectors [[1,2,3],[4,5,6],[7,8,9]].").completed()

    history = chat.get_chat_history()
    tool_calls = get_tool_calls(history)
    tool_responses = get_tool_responses(history)

    assert len(tool_calls) == 1
    assert tool_calls[0]["function"]["name"] == "add_list_of_vectors"
    assert tool_calls[0]["function"]["arguments"]["list_of_vectors"] == [[1, 2, 3], [4, 5, 6], [7, 8, 9]]

    assert len(tool_responses) == 1
    assert tool_responses[0]["name"] == "add_list_of_vectors"
    assert tool_responses[0]["content"] == "[12, 15, 18]"

def test_tool_with_dict(chat):
    if "qwen" not in os.environ.get("TEST_MODEL", "").lower():
        pytest.skip("Test only runs with Qwen models")

    chat.ask("Please use the provided tool to find the volume of a cube with dimensions 30 x 20 x 10.").completed()

    history = chat.get_chat_history()
    tool_calls = get_tool_calls(history)
    tool_responses = get_tool_responses(history)

    assert len(tool_calls) == 1
    assert tool_calls[0]["function"]["name"] == "calculate_volume"
    dimensions = tool_calls[0]["function"]["arguments"]["dimensions"]
    assert dimensions["width"] == 30
    assert dimensions["height"] == 20
    assert dimensions["depth"] == 10

    assert len(tool_responses) == 1
    assert tool_responses[0]["name"] == "calculate_volume"
    assert tool_responses[0]["content"] == "6000.0"