import os
from pathlib import Path

SYMLINK_PATH = Path("./model.gguf")


def pytest_markdown_docs_globals():
    import nobodywho

    # make symlink to TEST_MODEL, so we can use "./model.gguf" literal in docs
    model_path = os.environ.get("TEST_MODEL")
    assert isinstance(model_path, str)

    if not SYMLINK_PATH.exists():
        os.symlink(model_path, SYMLINK_PATH)

    return {
        "nobodywho": nobodywho,
        "Chat": nobodywho.Chat,
        "Model": nobodywho.Model,
        "SamplerPresets": nobodywho.SamplerPresets,
        "SamplerConfig": nobodywho.SamplerConfig,
    }


def pytest_sessionfinish(session, exitstatus):
    """Clean up the model.gguf symlink after test session."""
    if os.path.islink(SYMLINK_PATH):
        os.unlink(SYMLINK_PATH)
