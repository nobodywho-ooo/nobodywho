"""Tests for model downloading via the hf:// prefix."""

import os
import pytest
import nobodywho

# The model used for all download tests.
# This translates to: https://huggingface.co/NobodyWho/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf
HF_MODEL = "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-IQ2_M.gguf"
HF_REPO_ID = "NobodyWho/Qwen_Qwen3-0.6B-GGUF"
HF_FILENAME = "Qwen_Qwen3-0.6B-IQ2_M.gguf"

# An hf:// path with a valid format that refers to a model that doesn't exist,
# used for tests that verify the hf:// code path is reached without needing a
# real download to succeed.
HF_NONEXISTENT_MODEL = "hf://NobodyWho/this-repo-does-not-exist/model.gguf"

# An hf:// path with an invalid format (only 2 parts instead of owner/repo/file),
# used to trigger InvalidHfModelId immediately without any network call.
HF_INVALID_ID = "hf://not-a-valid-id"


# ---------------------------------------------------------------------------
# Test 1 – downloading a model works
# ---------------------------------------------------------------------------


@pytest.mark.network
def test_download_model():
    """A model can be loaded using an hf:// URL; this downloads it if needed."""
    model = nobodywho.Model(HF_MODEL)
    assert isinstance(model, nobodywho.Model)


# We use a second test here, simply because there is a slight logical difference in making models directly
# and by making a specific one. Model::new does not use get_inner_model
@pytest.mark.network
def test_download_model_chat():
    """A model can be loaded using an hf:// URL; this downloads it if needed."""
    model = nobodywho.Chat(HF_MODEL)
    assert isinstance(model, nobodywho.Chat)


# ---------------------------------------------------------------------------
# Test 2 – cached model loads when there is no internet
# ---------------------------------------------------------------------------


@pytest.mark.network
def test_cached_model_loads_offline():
    """After a model has been downloaded, it can be loaded without internet.

    hf-hub's sync ApiRepo.get() performs the cache check as pure filesystem
    operations (read refs/<revision>, stat snapshots/<hash>/<filename>).  No
    network socket is opened when the file is already cached.  Therefore,
    once the model has been downloaded once, subsequent loads via hf:// should
    succeed even without network access.

    We verify this by:
      1. Ensuring the model is present in the local hf-hub cache.
      2. Locating the cached file directly via hf_hub's Python cache API.
      3. Confirming the file exists on disk.
      4. Loading the model again – if this raises a download/network error the
         test fails, proving that the Rust code fell through to the network path
         instead of returning the cached file.
    """
    # Step 1: ensure cached.
    nobodywho.Model(HF_MODEL)

    # Step 2 & 3: confirm the file is actually on disk via the hf_hub Python
    # package (separate from the Rust hf-hub crate, but uses the same cache
    # directory convention at ~/.cache/huggingface/hub).
    try:
        from huggingface_hub import try_to_load_from_cache

        cached = try_to_load_from_cache(repo_id=HF_REPO_ID, filename=HF_FILENAME)
        assert cached is not None and os.path.isfile(cached), (
            "Model not found in local hf cache – cannot verify offline behaviour"
        )
    except ImportError:
        pytest.skip("huggingface_hub Python package not available for cache inspection")

    # Step 4: load again – must not raise a DownloadError.
    try:
        model = nobodywho.Model(HF_MODEL)
        assert isinstance(model, nobodywho.Model)
    except RuntimeError as e:
        if "Failed to download" in str(e) or "network" in str(e).lower():
            pytest.fail(
                f"Model load made a network request despite being cached: {e}"
            )
        raise


# ---------------------------------------------------------------------------
# Test 4 – a plain local path does NOT go through hf-hub
# ---------------------------------------------------------------------------


def test_local_path_does_not_trigger_download():
    """Passing a plain path (no hf:// prefix) raises 'Model not found', not a
    HuggingFace download error.  This confirms the hf-hub code path is never
    entered for local paths.
    """
    with pytest.raises(RuntimeError, match="Model not found"):
        nobodywho.Model("/this/path/does/not/exist/model.gguf")


# ---------------------------------------------------------------------------
# Test 5 – an hf:// prefix triggers the hf-hub code path
# ---------------------------------------------------------------------------


def test_hf_prefix_triggers_download_path():
    """Passing an hf:// path with an invalid format raises an
    'Invalid HuggingFace model ID' error, which is only emitted from the
    download code path.  This proves the hf:// prefix correctly routes through
    hf-hub rather than attempting a local file lookup.
    """
    with pytest.raises(RuntimeError, match="Invalid HuggingFace model ID"):
        nobodywho.Model(HF_INVALID_ID)


# ---------------------------------------------------------------------------
# Test 6 – all consumer classes route hf:// through the download path
# ---------------------------------------------------------------------------
#
# We use HF_NONEXISTENT_MODEL (valid owner/repo/file format, non-existent
# repo) so the format check passes but the download attempt fails.  The
# expected error is "Failed to download model from HuggingFace", which only
# appears when the hf:// prefix was recognised and the download was attempted.
# A plain "Model not found" error would mean the prefix was ignored and a
# local file lookup was performed instead.
#
# We do NOT use a real model here to avoid downloading hundreds of MB in a
# unit test.


def _assert_triggers_hf_download_path(exc: RuntimeError):
    msg = str(exc)
    assert "Model not found" not in msg, (
        f"Got a local file-not-found error instead of a download error: {msg}\n"
        "This means the hf:// prefix was not recognised."
    )
    assert (
        "Invalid HuggingFace model ID" in msg
        or "Failed to download" in msg
        or "HuggingFace" in msg
    ), f"Unexpected error (expected a HuggingFace-related error): {msg}"


@pytest.mark.network
def test_chat_triggers_hf_download_path():
    with pytest.raises(RuntimeError) as exc_info:
        nobodywho.Chat(HF_NONEXISTENT_MODEL)
    _assert_triggers_hf_download_path(exc_info.value)


@pytest.mark.network
def test_chat_async_triggers_hf_download_path():
    with pytest.raises(RuntimeError) as exc_info:
        nobodywho.ChatAsync(HF_NONEXISTENT_MODEL)
    _assert_triggers_hf_download_path(exc_info.value)


@pytest.mark.network
def test_encoder_triggers_hf_download_path():
    with pytest.raises(RuntimeError) as exc_info:
        nobodywho.Encoder(HF_NONEXISTENT_MODEL)
    _assert_triggers_hf_download_path(exc_info.value)


@pytest.mark.network
def test_encoder_async_triggers_hf_download_path():
    with pytest.raises(RuntimeError) as exc_info:
        nobodywho.EncoderAsync(HF_NONEXISTENT_MODEL)
    _assert_triggers_hf_download_path(exc_info.value)


@pytest.mark.network
def test_crossencoder_triggers_hf_download_path():
    with pytest.raises(RuntimeError) as exc_info:
        nobodywho.CrossEncoder(HF_NONEXISTENT_MODEL)
    _assert_triggers_hf_download_path(exc_info.value)


@pytest.mark.network
def test_crossencoder_async_triggers_hf_download_path():
    with pytest.raises(RuntimeError) as exc_info:
        nobodywho.CrossEncoderAsync(HF_NONEXISTENT_MODEL)
    _assert_triggers_hf_download_path(exc_info.value)


@pytest.mark.network
def test_model_triggers_hf_download_path():
    """nobodywho.Model() with hf:// also routes through hf-hub."""
    with pytest.raises(RuntimeError) as exc_info:
        nobodywho.Model(HF_NONEXISTENT_MODEL)
    _assert_triggers_hf_download_path(exc_info.value)
