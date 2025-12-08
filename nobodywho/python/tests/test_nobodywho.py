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


@pytest.fixture
def chat_async(model):
    return nobodywho.ChatAsync(
        model, system_prompt="You are a helpful assistant", allow_thinking=False
    )


@pytest.mark.asyncio
async def test_async_streaming(chat_async):
    """Test async streaming from demo_async.py"""
    prompt: str = "What is the capital of Denmark?"
    token_stream: nobodywho.TokenStreamAsync = chat_async.ask(prompt)

    tokens = []
    while token := await token_stream.next_token():
        tokens.append(token)

    response = "".join(tokens)
    assert len(response) > 0
    assert "copenhagen" in response.lower()


@pytest.mark.asyncio
async def test_async_completed(chat_async):
    """Test async complete from demo_async.py"""
    response_stream: nobodywho.TokenStreamAsync = chat_async.ask(
        "What is the capital of Denmark?"
    )
    response: str = await response_stream.completed()

    assert len(response) > 0
    assert "copenhagen" in response.lower()


def test_blocking_completed(chat):
    response_stream = chat.ask("What is the capital of Denmark?")
    response: str = response_stream.completed()
    assert "copenhagen" in response.lower()


@pytest.mark.asyncio
async def test_multiple_prompts(chat_async):
    """Test multiple sequential prompts like the demo loop"""
    prompts = ["Hello", "What is 2+2?", "Goodbye"]

    for prompt in prompts:
        response_stream: nobodywho.TokenStreamAsync = chat_async.ask(prompt)
        response = await response_stream.completed()
        assert len(response) > 0


def test_sync_iterator(chat):
    response_stream = chat.ask("What is the capital of Denmark?")
    response_str: str = ""
    for token in response_stream:
        response_str += token
        assert isinstance(token, str)
        assert len(token) > 0
    assert "copenhagen" in response_str.lower()


# Encoder tests
@pytest.fixture
def encoder_model():
    model_path = os.environ.get("TEST_EMBEDDINGS_MODEL")
    return nobodywho.Model(model_path, use_gpu_if_available=False)


@pytest.fixture
def encoder(encoder_model):
    return nobodywho.Encoder(encoder_model, n_ctx=1024)


def test_encoder_sync(encoder):
    """Test that encoder can generate embeddings using sync API"""
    embedding = encoder.encode("Test text for embedding.")

    assert isinstance(embedding, list), "Embedding should be a list"
    assert len(embedding) > 0, "Embedding should not be empty"
    assert all(isinstance(x, float) for x in embedding), (
        "All embedding values should be floats"
    )


@pytest.mark.asyncio
async def test_encoder_async():
    """Test that encoder can generate embeddings using async API"""
    model_path = os.environ.get("TEST_EMBEDDINGS_MODEL")
    model = nobodywho.Model(model_path, use_gpu_if_available=False)
    encoder_async = nobodywho.EncoderAsync(model, n_ctx=1024)

    embedding = await encoder_async.encode("Test text for embedding.")

    assert isinstance(embedding, list), "Embedding should be a list"
    assert len(embedding) > 0, "Embedding should not be empty"
    assert all(isinstance(x, float) for x in embedding), (
        "All embedding values should be floats"
    )


def test_cosine_similarity():
    """Test that cosine similarity function works"""
    vec1 = [1.0, 2.0, 3.0]
    vec2 = [4.0, 5.0, 6.0]

    similarity = nobodywho.cosine_similarity(vec1, vec2)
    assert isinstance(similarity, float), "Cosine similarity should return a float"

    # Test self-similarity
    self_sim = nobodywho.cosine_similarity(vec1, vec1)
    assert abs(self_sim - 1.0) < 0.001, "Self-similarity should be close to 1.0"


def test_cosine_similarity_error():
    """Test cosine similarity with mismatched vector lengths"""
    vec1 = [1.0, 2.0]
    vec2 = [1.0, 2.0, 3.0]

    with pytest.raises(ValueError):
        nobodywho.cosine_similarity(vec1, vec2)


# CrossEncoder tests
@pytest.fixture
def crossencoder_model():
    model_path = os.environ.get("TEST_CROSSENCODER_MODEL")
    return nobodywho.Model(model_path, use_gpu_if_available=False)


@pytest.fixture
def crossencoder(crossencoder_model):
    return nobodywho.CrossEncoder(crossencoder_model, n_ctx=4096)


def test_crossencoder_rank_sync(crossencoder):
    """Test that cross-encoder ranking works with sync API"""
    query = "What is the capital of France?"
    documents = [
        "Paris is the capital of France.",
        "Berlin is the capital of Germany.",
        "The weather is nice today.",
    ]

    scores = crossencoder.rank(query, documents)

    assert isinstance(scores, list), "Scores should be a list"
    assert len(scores) == len(documents), "Should return one score per document"
    assert all(isinstance(x, float) for x in scores), "All scores should be floats"


@pytest.mark.asyncio
async def test_crossencoder_rank_async():
    """Test that cross-encoder ranking works with async API"""
    model_path = os.environ.get("TEST_CROSSENCODER_MODEL")
    model = nobodywho.Model(model_path, use_gpu_if_available=False)
    crossencoder_async = nobodywho.CrossEncoderAsync(model, n_ctx=4096)

    query = "What is the capital of France?"
    documents = ["Paris is the capital of France.", "Berlin is the capital of Germany."]

    scores = await crossencoder_async.rank(query, documents)

    assert isinstance(scores, list), "Scores should be a list"
    assert len(scores) == len(documents), "Should return one score per document"
    assert all(isinstance(x, float) for x in scores), "All scores should be floats"


def test_crossencoder_rank_and_sort_sync(crossencoder):
    """Test that cross-encoder rank and sort works with sync API"""
    query = "What is the capital of France?"
    documents = [
        "Paris is the capital of France.",
        "Berlin is the capital of Germany.",
        "The weather is nice today.",
    ]

    ranked_docs = crossencoder.rank_and_sort(query, documents)

    assert isinstance(ranked_docs, list), "Ranked docs should be a list"
    assert len(ranked_docs) == len(documents), "Should return all documents"

    for doc, score in ranked_docs:
        assert isinstance(doc, str), "Document should be a string"
        assert isinstance(score, float), "Score should be a float"
        assert doc in documents, "Document should be from original list"


@pytest.mark.asyncio
async def test_crossencoder_rank_and_sort_async():
    """Test that cross-encoder rank and sort works with async API"""
    model_path = os.environ.get("TEST_CROSSENCODER_MODEL")
    model = nobodywho.Model(model_path, use_gpu_if_available=False)
    crossencoder_async = nobodywho.CrossEncoderAsync(model, n_ctx=4096)

    query = "What is the capital of France?"
    documents = ["Paris is the capital of France.", "Berlin is the capital of Germany."]

    ranked_docs = await crossencoder_async.rank_and_sort(query, documents)

    assert isinstance(ranked_docs, list), "Ranked docs should be a list"
    assert len(ranked_docs) == len(documents), "Should return all documents"

    for doc, score in ranked_docs:
        assert isinstance(doc, str), "Document should be a string"
        assert isinstance(score, float), "Score should be a float"
        assert doc in documents, "Document should be from original list"


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


def test_tool_parameter_description(model):
    # XXX: maybe there is a faster/better way of testing this behavior than running a full-ass LLM
    chat = nobodywho.Chat(model, tools=[reflarbicator, sparklify], allow_thinking=False)
    answer = chat.ask(
        "Please tell me the description of the 'unfloop' parameter of the reflarbicator tool"
    ).completed()
    assert "velocidensity" in answer


def test_tool_bad_parameters():
    with pytest.raises(TypeError):

        @nobodywho.tool(description="foobar", params={"b": "uh-oh"})
        def i_fucked_up(a: int) -> str:
            return "fuck"
