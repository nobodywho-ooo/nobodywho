from typing import Any, cast

import pytest

import nobodywho


def test_tts_rejects_invalid_backend() -> None:
    with pytest.raises(expected_exception=ValueError, match="backend"):
        nobodywho.Tts(source="missing-model", backend=cast(Any, "bad"))


def test_tts_requires_backend_for_unknown_sources() -> None:
    with pytest.raises(expected_exception=ValueError, match="backend"):
        nobodywho.Tts(source="missing-model")


def test_tts_rejects_invalid_device() -> None:
    with pytest.raises(expected_exception=ValueError, match="device"):
        nobodywho.Tts(source="missing-model", device=cast(Any, "bad"))
