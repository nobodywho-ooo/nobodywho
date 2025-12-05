# Python integration for Nobodywho

## Setting up
Creating virtual-env with `uv` should be enough.
```
uv venv
uv sync
```

## Building

We utilize [pyo3](https://github.com/PyO3/pyo3/) to generate the Python bindings from Rust code.
When building the library, use `maturin` to perform the conversion:
```
maturin develop --uv
```
Also, don't forget to create the Python type stubs (which unfortunately have to be generated separately):
```
cargo build
cargo run --bin make_stubs
```
Then you should be able to run `nobodywho`:
```
> source .venv/bin/python
> python
>>> import nobodywho
...
```