fn main() {
    // Emit `-undefined dynamic_lookup` which pyo3 needs on macOS:
    // https://pyo3.rs/v0.29.2/building-and-distribution.html#macos
    // https://github.com/PyO3/pyo3/issues/6400
    pyo3_build_config::add_extension_module_link_args();
}
