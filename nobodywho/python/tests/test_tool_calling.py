import os

import nobodywho
import pytest

import logging

logging.addLevelName(5, "TRACE")

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

    return nobodywho.Model("/home/hanshh/work/model_dir/functiongemma-270m-it-BF16.gguf", use_gpu_if_available=False)

@pytest.fixture
def chat(model):
    # if "qwen" in os.environ.get("TEST_MODEL", "").lower():
    #     return nobodywho.Chat(
    #         model, system_prompt="You are a helpful assistant", allow_thinking=False, tools=[set_intersection, multiply_strings, sparklify, reflarbicator, add_list_of_vectors, calculate_volume]
    #     )
    return nobodywho.Chat(
            model, system_prompt="You are a helpful assistant", allow_thinking=False, tools=[sparklify]
        )
    



def test_tool_construction():
    assert sparklify is not None
    assert isinstance(sparklify, nobodywho.Tool)
    assert sparklify("foobar") == "✨FOOBAR✨"


def test_tool_calling(chat):
    response: str = chat.ask("Please sparklify this word: 'julemand'").completed()
    assert "✨JULEMAND✨" in response




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
    response: str = chat.ask("Please sparklify this word: 'julemand'").completed()
    assert "✨JULEMAND✨" in response


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
    assert "Copenhagen" in resp2 and "sunny" in resp2.lower()


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

    response = chat.ask("Please sparklify this word: 'julemand'").completed()
    assert "✨JULEMAND✨" in response


# def test_tool_with_sets(chat):
#     if "qwen" in os.environ.get("TEST_MODEL", "").lower():
#         response = chat.ask("Please use the provided tool to find the intersection between the sets {12,5,7,3,4} and {12,9,5,3}").completed()
#         assert "12" in response and "5" in response and "3" in response 

# def test_tool_with_tuple(chat):
#     if "qwen" in os.environ.get("TEST_MODEL", "").lower():
#         response = chat.ask("Please use the provided tool to multiply the string BingBong by 3").completed()
#         assert "BingBongBingBongBingBong" in response

# def test_tool_with_nested_list(chat):
#     if "qwen" in os.environ.get("TEST_MODEL", "").lower():
#         response = chat.ask("Please use the provided tool to add the vectors [[1,2,3],[4,5,6],[7,8,9]].").completed()
#         assert "[12, 15, 18]" in response

# def test_tool_with_dict(chat):
#     if "qwen" in os.environ.get("TEST_MODEL", "").lower():
#         response = chat.ask("Please use the provided tool to find the volume of a cube with dimensions 30 x 20 x 10.").completed()
#         assert "6000" in response