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


# Embeddings tests
@pytest.fixture
def embeddings_model():
    model_path = os.environ.get("TEST_EMBEDDINGS_MODEL")
    return nobodywho.Model(model_path, use_gpu_if_available=False)


@pytest.fixture
def embeddings(embeddings_model):
    return nobodywho.Embeddings(embeddings_model, n_ctx=1024)


def test_embeddings_blocking(embeddings):
    """Test that embeddings can be generated using blocking API"""
    embedding = embeddings.embed_text_blocking("Test text for embedding.")

    assert isinstance(embedding, list), "Embedding should be a list"
    assert len(embedding) > 0, "Embedding should not be empty"
    assert all(isinstance(x, float) for x in embedding), (
        "All embedding values should be floats"
    )


@pytest.mark.asyncio
async def test_embeddings_async(embeddings):
    """Test that embeddings can be generated using async API"""
    embedding = await embeddings.embed_text("Test text for embedding.")

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


def test_crossencoder_rank_blocking(crossencoder):
    """Test that cross-encoder ranking works with blocking API"""
    query = "What is the capital of France?"
    documents = [
        "Paris is the capital of France.",
        "Berlin is the capital of Germany.",
        "The weather is nice today.",
    ]

    scores = crossencoder.rank_blocking(query, documents)

    assert isinstance(scores, list), "Scores should be a list"
    assert len(scores) == len(documents), "Should return one score per document"
    assert all(isinstance(x, float) for x in scores), "All scores should be floats"


@pytest.mark.asyncio
async def test_crossencoder_rank_async(crossencoder):
    """Test that cross-encoder ranking works with async API"""
    query = "What is the capital of France?"
    documents = ["Paris is the capital of France.", "Berlin is the capital of Germany."]

    scores = await crossencoder.rank(query, documents)

    assert isinstance(scores, list), "Scores should be a list"
    assert len(scores) == len(documents), "Should return one score per document"
    assert all(isinstance(x, float) for x in scores), "All scores should be floats"


def test_crossencoder_rank_and_sort_blocking(crossencoder):
    """Test that cross-encoder rank and sort works with blocking API"""
    query = "What is the capital of France?"
    documents = [
        "Paris is the capital of France.",
        "Berlin is the capital of Germany.",
        "The weather is nice today.",
    ]

    ranked_docs = crossencoder.rank_and_sort_blocking(query, documents)

    assert isinstance(ranked_docs, list), "Ranked docs should be a list"
    assert len(ranked_docs) == len(documents), "Should return all documents"

    for doc, score in ranked_docs:
        assert isinstance(doc, str), "Document should be a string"
        assert isinstance(score, float), "Score should be a float"
        assert doc in documents, "Document should be from original list"


@pytest.mark.asyncio
async def test_crossencoder_rank_and_sort_async(crossencoder):
    """Test that cross-encoder rank and sort works with async API"""
    query = "What is the capital of France?"
    documents = ["Paris is the capital of France.", "Berlin is the capital of Germany."]

    ranked_docs = await crossencoder.rank_and_sort(query, documents)

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
    response: str = chat.send_message(
        "Please sparklify this word: 'julemand'"
    ).collect_blocking()
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
    answer = chat.send_message(
        "Please tell me the description of the 'unfloop' parameter of the reflarbicator tool"
    ).collect_blocking()
    assert "velocidensity" in answer


def test_tool_bad_parameters():
    with pytest.raises(TypeError):

        @nobodywho.tool(description="foobar", params={"b": "uh-oh"})
        def i_fucked_up(a: int) -> str:
            return "fuck"
