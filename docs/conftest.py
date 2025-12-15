import os
from pathlib import Path

MODEL_SYMLINK = Path("./model.gguf")
EMBEDDING_SYMLINK = Path("./embedding-model.gguf")
RERANKER_SYMLINK = Path("./reranker-model.gguf")


def pytest_markdown_docs_globals():
    import nobodywho

    # make symlink to TEST_MODEL, so we can use "./model.gguf" literal in docs
    model_path = os.environ.get("TEST_MODEL")
    assert isinstance(model_path, str)

    if not MODEL_SYMLINK.exists():
        os.symlink(model_path, MODEL_SYMLINK)

    # make symlink to TEST_EMBEDDING_MODEL, so we can use "./embedding-model.gguf" literal in docs
    embedding_model_path = os.environ.get("TEST_EMBEDDINGS_MODEL")
    if embedding_model_path and not EMBEDDING_SYMLINK.exists():
        os.symlink(embedding_model_path, EMBEDDING_SYMLINK)

    # make symlink to TEST_RERANKER_MODEL, so we can use "./reranker-model.gguf" literal in docs
    reranker_model_path = os.environ.get("TEST_CROSSENCODER_MODEL")
    if reranker_model_path and not RERANKER_SYMLINK.exists():
        os.symlink(reranker_model_path, RERANKER_SYMLINK)

    return {
        "nobodywho": nobodywho,
        "Chat": nobodywho.Chat,
        "Model": nobodywho.Model,
        "SamplerPresets": nobodywho.SamplerPresets,
        "SamplerConfig": nobodywho.SamplerConfig,
        "Encoder": nobodywho.Encoder,
        "EncoderAsync": nobodywho.EncoderAsync,
        "CrossEncoder": nobodywho.CrossEncoder,
        "CrossEncoderAsync": nobodywho.CrossEncoderAsync,
        "cosine_similarity": nobodywho.cosine_similarity,
        "tool": nobodywho.tool,
    }


def pytest_sessionfinish(session, exitstatus):
    """Clean up symlinks after test session."""
    for symlink in [MODEL_SYMLINK, EMBEDDING_SYMLINK, RERANKER_SYMLINK]:
        if os.path.islink(symlink):
            os.unlink(symlink)
