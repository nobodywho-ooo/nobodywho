{ pkgs, fetchurl, rustPlatform, llvmPackages_12, cmake, vulkan-headers, vulkan-loader, vulkan-tools, shaderc, mesa }:

let
  nobodywho = rustPlatform.buildRustPackage {
    pname = "nobodywho";
    version = "1.0.0";
    src = ./..;
    nativeBuildInputs = [
      llvmPackages_12.bintools
      cmake
      rustPlatform.bindgenHook
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
        "godot-0.2.4" = "sha256-gKwJGLKfOkRZS1P2erfixAHg0l/X7U0Mhf4SZxxGrZg=";
        "llama-cpp-2-0.1.103" = "sha256-T8qxxAC0ygF655EzODIpDjIKS0vRMe68e5rJcP1+PDo=";
      };
    };
    env.TEST_MODEL = fetchurl {
      name = "qwen2.5-1.5b-instruct-q4_0.gguf";
      url = "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_0.gguf";
      hash = "sha256-3NgZ/wlIUsOPq6aHPY/wydUerbKERTnlIEKuXWR7v9s=";
    };
    env.TEST_EMBEDDINGS_MODEL = fetchurl {
      name = "bge-small-en-v1.5-q8_0.gguf";
      url = "https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/main/bge-small-en-v1.5-q8_0.gguf";
      sha256 = "sha256-7Djo2hQllrqpExJK5QVQ3ihLaRa/WVd+8vDLlmDC9RQ=";
    };
    checkPhase = ''
      cargo test -- --test-threads=1 --nocapture
    '';
    doCheck = true;
  };

in {
  nobodywho = nobodywho;
  packages = {
    inherit nobodywho;
  };
}
