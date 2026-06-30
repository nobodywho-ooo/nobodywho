check: fmt clippy regen-python regen-flutter ruff

fmt:
    cd nobodywho && cargo fmt --all

clippy:
    cd nobodywho/core && cargo clippy --no-deps -- -D warnings

regen-python:
    cd nobodywho/python && cargo build && cargo run --bin make_stubs && uv run ruff format nobodywho.pyi

regen-flutter:
    cd nobodywho/flutter/nobodywho && dart run tool/doctest.dart ../../../docs/docs-flutter --generate-only

ruff:
    cd nobodywho/python && uv run ruff format && uv run ruff check
