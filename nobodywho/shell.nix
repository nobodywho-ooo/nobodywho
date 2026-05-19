{
  pkgs ? import <nixpkgs> { },
  ...
}:

let
  test-models = pkgs.callPackage ./models.nix { };
in
pkgs.mkShell {
  env = {
    # Bindgen probes $LIBCLANG_PATH for the dynamic library. The filename
    # differs by platform (.so on Linux, .dylib on macOS); point at the
    # directory and let bindgen pick the right one rather than hardcoding
    # the extension.
    LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";

    # walkingeyerobot's Emscripten fork looks up wasm-bindgen-cli via the
    # `WASM_BINDGEN` key of its ~/.emscripten config file. The standard
    # env-var override is `EM_<KEY>` — set it here so emcc finds the cli
    # without us having to write an Emscripten config file.
    #
    # Pointing at a locally-built patched cli at /tmp/wbg-patched/bin/
    # while we iterate on wasm-bindgen interpreter fixes. Source clone
    # lives at /Users/user/git/wasm-bindgen (branch 0.2.121 + local
    # patches in crates/cli-support/src/interpreter/mod.rs). Once the
    # patches land upstream, switch this back to the Nix-store cli:
    #   EM_WASM_BINDGEN = "${pkgs.wasm-bindgen-cli}/bin/wasm-bindgen";
    EM_WASM_BINDGEN = "/tmp/wbg-patched/bin/wasm-bindgen";
    LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
      pkgs.vulkan-loader
      pkgs.gcc.cc.lib
    ];
  }
  // (removeAttrs test-models [ "override" "overrideDerivation" ]);

  packages = [
    pkgs.cmake
    pkgs.clang
    pkgs.rustup

    # these are the dependencies required by llama.cpp to build for vulkan
    # (these packages were discovered by looking at the nix source code in ggerganov/llama.cpp)
    pkgs.vulkan-headers
    pkgs.vulkan-loader
    pkgs.shaderc

    # for android
    # android-sdk
    # pkgs.openjdk11-bootstrap

    # for mkdocs
    pkgs.python3Packages.mkdocs
    pkgs.python3Packages.regex
    pkgs.python3Packages.mkdocs-material

    # flutter
    pkgs.flutter
    pkgs.flutter_rust_bridge_codegen

    # python
    pkgs.python3
    pkgs.maturin
    pkgs.python3Packages.uv
    pkgs.python3Packages.pytest
    pkgs.python3Packages.pytest-asyncio

    # node.js (for napi-rs bindings)
    pkgs.nodejs

    # Emscripten (walkingeyerobot fork with -sWASM_BINDGEN, pulled in via
    # the overlay in flake.nix). Used to build nobodywho-js against
    # wasm32-unknown-emscripten so the published wasm needs no setup.mjs
    # bootstrap on the JS side.
    pkgs.emscripten
    # wasm-bindgen-cli pinned to a version compatible with the fork; emcc
    # shells out to it during link when -sWASM_BINDGEN is set.
    pkgs.wasm-bindgen-cli
  ];
  shellHook = ''
    ulimit -n 2048
  '';
}
