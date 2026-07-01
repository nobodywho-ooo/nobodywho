# Python integration for Nobodywho

## Setting up
Creating virtual-env with `uv` should be enough.
```
uv venv
uv sync
```
We ignore packages published in the last two weeks for security reasons. (You know).

## Building

We utilize [pyo3](https://github.com/PyO3/pyo3/) to generate the Python bindings from Rust code.
When building the library, use `maturin` to perform the conversion:
```
maturin develop --uv
```
Also, don't forget to create and format the Python type stubs (which unfortunately have to be generated separately):
```
cargo build
cargo run --bin make_stubs
uv run ruff format nobodywho.pyi
```
Then you should be able to run `nobodywho`:
```
> source .venv/bin/python
> python
>>> import nobodywho
...
```

## Static checks

Run formatting, linting, and type checking from this directory:

```shell
uv run ruff format --check
uv run ruff check
uv run ty check
```

## Testing

We use pytest for testing.

Assuming that you've already activated the virtual environment:

To run the tests:
```shell
python3 -m pytest
```

We also test all codeblocks in the markdown documentation:

```shell
python3 -m pytest --markdown-docs ../../docs --markdown-docs-syntax=superfences --log-cli-level=9
```
