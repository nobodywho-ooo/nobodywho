import os
from pathlib import Path

MODEL_SYMLINK = Path("./model.gguf")
EMBEDDING_SYMLINK = Path("./embedding-model.gguf")
RERANKER_SYMLINK = Path("./reranker-model.gguf")
VISION_MODEL_SYMLINK = Path("./vision-model.gguf")
PROJECTION_MODEL_SYMLINK = Path("./projection_model.gguf")
DOG_IMAGE_SYMLINK = Path("./dog.png")
PENGUIN_IMAGE_SYMLINK = Path("./penguin.png")


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

    # make symlink to TEST_VISION_MODEL, so we can use "./vision-model.gguf" literal in docs
    vision_model_path = os.environ.get("TEST_VISION_MODEL")
    if vision_model_path and not VISION_MODEL_SYMLINK.exists():
        os.symlink(vision_model_path, VISION_MODEL_SYMLINK)

    # make symlink to TEST_MMPROJ_MODEL, so we can use "./projection_model.gguf" literal in docs
    mmproj_path = os.environ.get("TEST_MMPROJ_MODEL")
    if mmproj_path and not PROJECTION_MODEL_SYMLINK.exists():
        os.symlink(mmproj_path, PROJECTION_MODEL_SYMLINK)

    # make symlinks for test images used in vision docs
    tests_img_dir = Path(__file__).parent.parent / "nobodywho" / "python" / "tests" / "img"
    if (tests_img_dir / "dog.png").exists() and not DOG_IMAGE_SYMLINK.exists():
        os.symlink(tests_img_dir / "dog.png", DOG_IMAGE_SYMLINK)
    if (tests_img_dir / "penguin.png").exists() and not PENGUIN_IMAGE_SYMLINK.exists():
        os.symlink(tests_img_dir / "penguin.png", PENGUIN_IMAGE_SYMLINK)

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
        "Text": nobodywho.Text,
        "Image": nobodywho.Image,
        "Prompt": nobodywho.Prompt,
    }


def pytest_sessionfinish(session, exitstatus):
    """Clean up symlinks after test session."""
    for symlink in [
        MODEL_SYMLINK,
        EMBEDDING_SYMLINK,
        RERANKER_SYMLINK,
        VISION_MODEL_SYMLINK,
        PROJECTION_MODEL_SYMLINK,
        DOG_IMAGE_SYMLINK,
        PENGUIN_IMAGE_SYMLINK,
    ]:
        if os.path.islink(symlink):
            os.unlink(symlink)
