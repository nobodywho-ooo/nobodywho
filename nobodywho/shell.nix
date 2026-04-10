{
  pkgs ? import <nixpkgs> { },
  ...
}:

let
  test-models = pkgs.callPackage ./models.nix { };
in
pkgs.mkShell {
  env = {
    LIBCLANG_PATH = "${pkgs.libclang.lib}/lib/libclang.so";
    LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
      pkgs.vulkan-loader
      pkgs.gcc.cc.lib
    ];
    # Tells the emscripten fork where to find wasm-bindgen CLI
    # (used by -sWASM_BINDGEN at link time)
    EM_WASM_BINDGEN = "${pkgs.wasm-bindgen-cli}/bin/wasm-bindgen";
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

    # web/wasm (emscripten + wasm-bindgen)
    pkgs.emscripten
    pkgs.wasm-bindgen-cli

    # python
    pkgs.python3
    pkgs.maturin
    pkgs.python3Packages.uv
    pkgs.python3Packages.pytest
    pkgs.python3Packages.pytest-asyncio
  ];
  shellHook = ''
    ulimit -n 2048
    # Emscripten's sysroot isn't pre-populated by the Nix package,
    # so we need to generate it on first use for wasm builds to work.
    embuilder build sysroot
  '';
}
