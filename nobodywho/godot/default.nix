{ fetchurl, rustPlatform, libclang, llvmPackages_12, stdenv, lib, cmake, vulkan-headers, vulkan-loader, vulkan-tools, shaderc, mesa, git, curl, rustfmt }:


rustPlatform.buildRustPackage {
  pname = "nobody";
  version = "0.0.0";
  src = ./..;
  buildAndTestSubdir = "godot";
  nativeBuildInputs = [
    git
    llvmPackages_12.bintools
    cmake
    rustPlatform.bindgenHook
    rustPlatform.cargoBuildHook
    vulkan-headers
    vulkan-loader
    shaderc
    vulkan-tools
    mesa
    rustfmt
  ];
  buildInputs = [
    curl.dev
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
      "godot-0.2.4" = "sha256-XcvYeBE9r6i9qqwg6PZMNQmBDLYHezQwdeN+f90GJ8E=";
      "llama-cpp-2-0.1.107" = "sha256-vVofBVBlxmKDcypTJGQxOuB5EJ8azwTU/wWiFiSQw1w=";
    };
  };
  env.TEST_MODEL = fetchurl {
    name = "Qwen_Qwen3-0.6B-Q4_0.gguf";
    url = "https://huggingface.co/bartowski/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_0.gguf";
    hash = "sha256-S3jY48YZds67eO9a/+GdDsp1sbR+xm9hOloyRUhHWNU=";
  };

  env.TEST_EMBEDDINGS_MODEL = fetchurl {
    name = "bge-small-en-v1.5-q8_0.gguf";
    url = "https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/main/bge-small-en-v1.5-q8_0.gguf";
    sha256 = "sha256-7Djo2hQllrqpExJK5QVQ3ihLaRa/WVd+8vDLlmDC9RQ=";
  };

  checkPhase = ''
    RUST_BACKTACE=1 cargo test -- --test-threads=1 --nocapture
  '';
  doCheck = true;
}
