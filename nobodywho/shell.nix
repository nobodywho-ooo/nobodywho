{
  pkgs ? import <nixpkgs> { },
  ...
}:

let
  unity_version = "6000.0.47f1";
  unity-editor = pkgs.writeShellScriptBin "unity-editor" ''
    ${pkgs.lib.getExe pkgs.unityhub.fhsEnv} ~/Unity/Hub/Editor/${unity_version}/Editor/Unity "$@"
  '';
in
pkgs.mkShell {
  env.LIBCLANG_PATH = "${pkgs.libclang.lib}/lib/libclang.so";
  env.TEST_MODEL = pkgs.fetchurl {
    name = "Qwen_Qwen3-0.6B-Q4_K_M.gguf";
    url = "https://huggingface.co/bartowski/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf";
    sha256 = "sha256-ms/B4AExHzS0JSABtiby5GbVkqQgZfZlcb/zeQ1OGxQ=";
  };
  env.TEST_EMBEDDINGS_MODEL = pkgs.fetchurl {
    name = "bge-small-en-v1.5-q8_0.gguf";
    url = "https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/main/bge-small-en-v1.5-q8_0.gguf";
    sha256 = "sha256-7Djo2hQllrqpExJK5QVQ3ihLaRa/WVd+8vDLlmDC9RQ=";
  };
  env.TEST_CROSSENCODER_MODEL = pkgs.fetchurl {
    name = "bge-reranker-v2-m3-Q8_0.gguf";
    url = "https://huggingface.co/gpustack/bge-reranker-v2-m3-GGUF/resolve/main/bge-reranker-v2-m3-Q8_0.gguf";
    sha256 = "sha256-pDx8mxGkwVF+W/lRUZYOFiHRty96STNksB44bPGqodM=";
  };

  packages = [
    pkgs.cmake
    pkgs.clang
    pkgs.rustup

    # Unity
    unity-editor
    pkgs.unityhub
    pkgs.libxml2
    pkgs.just
    # lsp dependencies
    pkgs.dotnet-sdk_9
    pkgs.csharpier
    # used to make good stack traces
    pkgs.gdb
    pkgs.lldb

    # these are the dependencies required by llama.cpp to build for vulkan
    # (these packages were discovered by looking at the nix source code in ggerganov/llama.cpp)
    pkgs.vulkan-headers
    pkgs.vulkan-loader
    pkgs.shaderc

    # for android
    # android-sdk
    # pkgs.openjdk11-bootstrap

    # for mkdocs
    pkgs.mkdocs
    pkgs.python312Packages.regex
    pkgs.python312Packages.mkdocs-material

    # flutter
    pkgs.flutter
    pkgs.flutter_rust_bridge_codegen

    # python
    pkgs.python3
    pkgs.maturin
    pkgs.python3Packages.pytest
    pkgs.python3Packages.pytest-asyncio
  ];
  shellHook = ''
    ulimit -n 2048
  '';
}
