
# Nobodywho Flutter bindings

Here is four folders:

## `rust/`

This dir contains the rust code which uses [flutter_rust_bridge](https://github.com/fzyzcjy/flutter_rust_bridge) macros to do code-generation.

The build.rs file deals with code generation, outputting both flutter and dart code.
For this reason it expects `flutter` to be on the path.
The `NOBODYWHO_SKIP_CODEGEN` environment variable can be used to skip codegen if needed.

Whenever the `rust` crate is built, it overwrites the code-generated files in the repo.

## `nobodywho_dart/`

This pure-dart package contains the generated dart code for binding into the rust crate.

It also contains a tiny bit of dart code to wrap tool calling functions.

## `nobodywho_flutter/`

This flutter plugin depends on and re-exports the `nobodywho_dart` package, and also contains platform-specific build scripts for including pre-built binary artifacts (so the end-user doesn't need the build-time dependencies of rust and llama.cpp).


