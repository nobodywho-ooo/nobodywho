
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

This also contains a bunch of unit tests to check that the bindings actually work.
They can be run using `dart test`, but dart needs to be able to find the dynamlic library of nobodywho to run them. I usually do something like this:

**On Linux**:
```rust
LD_LIBRARY_PATH=../../target/debug/ dart test
```
**On Mac**:
It is a bit more difficult, because for some reason, `flutter_rust_bridge` does not respect the `DYLD_LIBRARY_PATH` which is made for this and rather opts for something on the path:
```
*.framework/library_name
```
So temporary solution is to create a symlink with:
```bash
cd flutter/rust
cargo build
cd flutter/nobodywho_dart
mkdir -p nobodywho_flutter.framework
ln -sf ../../../target/debug/libnobodywho_flutter.dylib nobodywho_flutter.framework/nobodywho_flutter
```
(Opt for the release you want, it can be debug or something else.)

## `nobodywho_flutter/`

This flutter plugin depends on and re-exports the `nobodywho_dart` package, and also contains platform-specific build scripts for including pre-built binary artifacts (so the end-user doesn't need the build-time dependencies of rust and llama.cpp).

In general, the build scripts (CMakeLists.txt, Podspec, gradle/ktl files) will try these three things:

1. check the environment variable `NOBODYWHO_FLUTTER_LIB_PATH`
2. check for libnobodywho_flutter.so in a parent cargo target dir (`../../../target/release/libnobodywho_flutter.so`), useful during development
3. figure out the verison number of the flutter package, and download the corresponding dynamlic lib from github releases (WIP)

## Building for Android

The nix file `android.nix` sets up a nix devshell, which has all of the required android sdks installed, and the environment variables set up to point in the right places.

You can activate it by running `nix develop '.#android'`.

It is tested and works on x86_64-linux, and the github actions CI environment also tests that building the flutter example application inside this devshell works.
I assume that it will also work on MacOS, with a few tweaks (e.g. setting the correct dynamic library extension, including a few macos "Frameworks" in the build env).
