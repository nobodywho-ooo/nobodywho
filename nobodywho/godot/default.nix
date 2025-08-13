{
  pkgs,
  fetchurl,
  rustPlatform,
  llvmPackages_12,
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
    src = ./..;
    buildAndTestSubdir = "godot";
    nativeBuildInputs = [
      llvmPackages_12.bintools
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
        "gdextension-api-0.2.2" = "sha256-gaxM73OzriSDm6tLRuMTOZxCLky9oS1nq6zTsm0g4tA=";
        "godot-0.2.4" = "sha256-5Kh1j3OpUetuE9qNK85tpZTj8m0Y30CX4okll4TZ9Xc=";
        "gbnf-0.2.2" = "sha256-yxJhq0HdDdPnIwpIBi1Oj8LBEUDqbecIF2ATE3T+wXk=";
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
