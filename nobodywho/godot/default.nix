{
  pkgs,
  lib,
  fetchurl,
  rustPlatform,
  llvmPackages,
  cmake,
  vulkan-headers,
  vulkan-loader,
  vulkan-tools,
  shaderc,
  mesa,
}:

rec {
  nobodywho-godot = rustPlatform.buildRustPackage {
    pname = "nobodywho-godot";
    version = "0.0.0";

    # filter out stuff that's not godot or core
    src = lib.cleanSourceWith {
      src = ../.;
      filter =
        path: type:
        !(lib.hasInfix "flutter" path)
        && !(lib.hasInfix "unity" path)
        && !(lib.hasInfix "integration-test" path);
    };
    # patch the workspace to only include members we have in the filtered source
    postPatch = ''
          substituteInPlace Cargo.toml \
            --replace-fail 'members = [
          "core",
          "godot",
          "unity",
          "flutter/rust"
      ]' 'members = [
          "core",
          "godot"
      ]'
    '';

    buildAndTestSubdir = "godot";
    nativeBuildInputs = [
      pkgs.tree
      llvmPackages.bintools
      cmake
      rustPlatform.bindgenHook
      rustPlatform.cargoBuildHook
      vulkan-headers
      vulkan-loader
      shaderc
      vulkan-tools
      mesa
      pkgs.git
    ];
    buildInputs = [
      vulkan-loader
      vulkan-headers
      shaderc
      vulkan-tools
      mesa
    ];
    cargoLock = {
      lockFile = ../Cargo.lock;
      outputHashes = {
        "llama-cpp-2-0.1.123" = "sha256-j69yaWBiZ9ERlNdi9sD1K4tQjmDYbzoYwr8TU/+8D2A=";
      };
    };
    env.TEST_MODEL = fetchurl {
      name = "Qwen_Qwen3-0.6B-Q4_K_M.gguf";
      url = "https://huggingface.co/bartowski/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf";
      sha256 = "sha256-ms/B4AExHzS0JSABtiby5GbVkqQgZfZlcb/zeQ1OGxQ=";
    };
    env.TEST_EMBEDDINGS_MODEL = fetchurl {
      name = "bge-small-en-v1.5-q8_0.gguf";
      url = "https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/main/bge-small-en-v1.5-q8_0.gguf";
      sha256 = "sha256-7Djo2hQllrqpExJK5QVQ3ihLaRa/WVd+8vDLlmDC9RQ=";
    };
    env.TEST_CROSSENCODER_MODEL = fetchurl {
      name = "bge-reranker-v2-m3-Q8_0.gguf";
      url = "https://huggingface.co/gpustack/bge-reranker-v2-m3-GGUF/resolve/main/bge-reranker-v2-m3-Q8_0.gguf";
      sha256 = "sha256-pDx8mxGkwVF+W/lRUZYOFiHRty96STNksB44bPGqodM=";
    };
    checkPhase = ''
      cargo test -- --test-threads=1 --nocapture
    '';
    doCheck = true;
  };

  integration-test = pkgs.callPackage ./integration-test { inherit nobodywho-godot; };

  run-integration-test =
    pkgs.runCommand "checkgame"
      {
        nativeBuildInputs = [ mesa ];
      }
      ''
        cd ${integration-test}
        export HOME=$TMPDIR
        ./game --headless
        touch $out
      '';
}
