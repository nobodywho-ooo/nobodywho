{ fetchurl, rustPlatform, libclang, llvmPackages_12, stdenv, lib, cmake, vulkan-headers, vulkan-loader, vulkan-tools, shaderc, mesa }:


rustPlatform.buildRustPackage {
  pname = "nobody";
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
    mesa.drivers
  ];
  buildInputs = [
    vulkan-loader
    vulkan-headers
    shaderc
    vulkan-tools
    mesa.drivers
  ];
  cargoLock = {
    lockFile = ../Cargo.lock;
    outputHashes = {
      "gdextension-api-0.2.1" = "sha256-YkMbzObJGnmQa1XGT4ApRrfqAeOz7CktJrhYks8z0RY=";
      "godot-0.2.4" = "sha256-mQcI5PO1rTYeKg+2FH7tSbq6+nk3R5kqHGZ+f797e34=";
      "llama-cpp-2-0.1.101" = "sha256-vb5eRegcR+qPaN+kAt4BUS0COnjvqgf7cQAZramYd/8=";
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
}
