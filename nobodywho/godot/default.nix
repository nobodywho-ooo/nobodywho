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
  callPackage,
}:

let
  models = callPackage ../models.nix { };
in
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
        && !(lib.hasInfix "python" path)
        && !(lib.hasInfix "integration-test" path);
    };
    # patch the workspace to only include members we have in the filtered source
    postPatch = ''
          substituteInPlace Cargo.toml \
            --replace-fail 'members = [
          "core",
          "godot",
          "unity",
          "flutter/rust",
          "python",
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
        "llama-cpp-2-0.1.127" = "sha256-pkFRjNUADPd+PWpjK9Uw+qQIe7h2KNaLyISykdLCOi0=";
      };
    };
    env.TEST_MODEL = models.TEST_MODEL;
    env.TEST_EMBEDDINGS_MODEL = models.TEST_EMBEDDINGS_MODEL;
    env.TEST_CROSSENCODER_MODEL = models.TEST_CROSSENCODER_MODEL;
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
