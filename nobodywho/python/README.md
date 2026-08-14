# Python integration for Nobodywho

## Setting up
Create the virtual environment and install the locked dependencies:
```
uv sync
```
We ignore packages published in the last two weeks for security reasons. (You know).

## Building

We utilize [pyo3](https://github.com/PyO3/pyo3/) to generate the Python bindings from Rust code.
When building the library, use `maturin` to perform the conversion:
```
uv run maturin develop --uv
```
Also, don't forget to create and format the Python type stubs (which unfortunately have to be generated separately):
```
cargo build
cargo run --bin make_stubs
uv run ruff format nobodywho.pyi
```
Then you should be able to run `nobodywho`:
```
uv run python
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

We use pytest for testing:
```shell
uv run pytest
```

We also test all codeblocks in the markdown documentation:

```shell
uv run pytest --markdown-docs ../../docs --markdown-docs-syntax=superfences --log-cli-level=9
```
