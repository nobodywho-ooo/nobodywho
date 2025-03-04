{ pkgs, nobodywho, fetchurl, rustPlatform, libclang, llvmPackages_12, godot_4, godot_4-export-templates, stdenv, lib, cmake, vulkan-headers, vulkan-loader, vulkan-tools, shaderc, mesa }:

let
  godot = rustPlatform.buildRustPackage {
    pname = "godot";
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
    ];
    buildInputs = [
      nobodywho
      vulkan-loader
      vulkan-headers
      shaderc
      vulkan-tools
      mesa
    ];
    cargoLock = {
      lockFile = ../Cargo.lock;
      outputHashes = {
        "gdextension-api-0.2.1" = "sha256-YkMbzObJGnmQa1XGT4ApRrfqAeOz7CktJrhYks8z0RY=";
        "godot-0.2.2" = "sha256-6q7BcQ/6WvzJdVmyAVGPMtuIDJFYKaRrkv3/JQBi11M=";
        "llama-cpp-2-0.1.90" = "sha256-SUY4Hb2DAjDonQMEIyKXoXRygxBh/M+CeyQBviqg46g=";
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

  integration-test = pkgs.callPackage ./integration-test {
    inherit godot;  # Pass the godot package we just built
  };

  run-integration-test = pkgs.runCommand "checkgame" {
    nativeBuildInputs = [ mesa.drivers ];
  } ''
    cd ${integration-test}
    export HOME=$TMPDIR
    ./game --headless
    touch $out
  '';

in {
  godot = godot;
  checks = {
    godot-integration-test = run-integration-test;
  };
}
