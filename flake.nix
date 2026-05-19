{
  nixConfig = {
    extra-substituters = "https://cache.nixos.org https://hydra.nixos.org https://nobodywho.cachix.org https://nix-community.cachix.org";
    extra-trusted-public-keys = "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY= hydra.nixos.org-1:CNHJZBh9K4tP3EKF6FkkgeVYsS3ohTl+oS0Qa8bezVs= nobodywho.cachix.org-1:VdcBFxLwfO1L23J973e4UolSnt3QlSZvT1E23+L+9WU= nix-community.cachix.org-1:mB9FSh9qf2dCimDSUo8Zy7bkq5CX+/rkCWyvRCYg3Fs=";
    extra-experimental-features = "nix-command flakes";
  };
  description = "NobodyWho - a godot plugin for NPC dialogue with local LLMs";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    android-nixpkgs.url = "github:tadfisher/android-nixpkgs";
    crate2nix.url = "github:nix-community/crate2nix";
    crate2nix.inputs.nixpkgs.follows = "nixpkgs";
    # walkingeyerobot's emscripten fork with -sWASM_BINDGEN support
    # (draft PR: emscripten-core/emscripten#23493). Needed to build
    # nobodywho-js against wasm32-unknown-emscripten with auto-generated
    # JS bindings — emcc invokes wasm-bindgen-cli during link.
    emscripten-wbg-src = {
      url = "github:walkingeyerobot/emscripten/wbg-walkingeyerobot";
      flake = false;
    };
  };
  outputs =
    {
      nixpkgs,
      flake-utils,
      android-nixpkgs,
      crate2nix,
      emscripten-wbg-src,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = (
          import nixpkgs {
            inherit system;
            config = {
              android_sdk.accept_license = true;
            };
            overlays = [
              # Override emscripten with walkingeyerobot's fork that adds
              # -sWASM_BINDGEN. Pin wasm-bindgen-cli to 0.2.121 to match
              # the wasm-bindgen crate version that's transitively pulled
              # in by web-sys / js-sys / wasm-bindgen-futures (see
              # nobodywho/Cargo.lock). The CLI and crate schemas must
              # match exactly — wasm-bindgen errors out at link time
              # otherwise.
              #
              # Caveat: this CLI is the stock 0.2.121, which panics on
              # Emscripten-emitted wasm in `interpret_descriptor`
              # (`no entry found for key`). For the Emscripten build path
              # we shadow it with a locally-built patched CLI; see
              # nobodywho/shell.nix's EM_WASM_BINDGEN env var. This
              # entry is kept so nix-flake consumers that only need
              # native wasm-bindgen (e.g. wasm32-unknown-unknown builds
              # via the wasi-sdk path) still get a working binary.
              (final: prev: {
                wasm-bindgen-cli = prev.buildWasmBindgenCli rec {
                  src = prev.fetchCrate {
                    pname = "wasm-bindgen-cli";
                    version = "0.2.121";
                    hash = "sha256-ZOMgFNOcGkO66Jz/Z83eoIu+DIzo3Z/vq6Z5g6BDY/w=";
                  };
                  cargoDeps = prev.rustPlatform.fetchCargoVendor {
                    inherit src;
                    inherit (src) pname version;
                    hash = "sha256-DPdCDPTAPBrbqLUqnCwQu1dePs9lGg85JCJOCIr9qjU=";
                  };
                };
                emscripten = prev.emscripten.overrideAttrs (old: {
                  src = emscripten-wbg-src;
                  # The fork is merged with upstream main (past 5.0.0) so it
                  # expects LLVM 23, but nixpkgs ships LLVM 21. The stock
                  # derivation's buildPhase seds "22" -> "21.1"; also handle 23.
                  buildPhase = builtins.replaceStrings
                    [ "EXPECTED_LLVM_VERSION = 22" ]
                    [ "EXPECTED_LLVM_VERSION = 23" ]
                    old.buildPhase;
                });
              })
            ];
          }
        );

        workspace = pkgs.callPackage ./nobodywho/workspace.nix { inherit crate2nix; };

        # python stuff
        nobodywho-python = pkgs.callPackage ./nobodywho/python { inherit workspace; };

        # flutter stuff
        nobodywho-flutter = workspace.workspaceMembers.nobodywho-flutter.build;
        flutter_tests = pkgs.callPackage ./nobodywho/flutter/nobodywho {
          nobodywho_flutter_rust = nobodywho-flutter;
        };

        # cargo tests
        test-models = pkgs.callPackage ./nobodywho/models.nix { };
        nobodywho-tested = workspace.workspaceMembers.nobodywho.build.override {
          runTests = true;
          testPreRun = ''
            export TEST_MODEL=${test-models.TEST_MODEL}
            export TEST_EMBEDDINGS_MODEL=${test-models.TEST_EMBEDDINGS_MODEL}
            export TEST_CROSSENCODER_MODEL=${test-models.TEST_CROSSENCODER_MODEL}


          '';
        };

        # godot stuff
        nobodywho-godot = workspace.workspaceMembers.nobodywho-godot.build;
        godot-integration-test = pkgs.callPackage ./nobodywho/godot/integration-test {
          inherit nobodywho-godot;
        };
        run-godot-integration-test =
          pkgs.runCommand "checkgame"
            {
              nativeBuildInputs = [ pkgs.mesa ];
            }
            ''
              cd ${godot-integration-test}
              export HOME=$TMPDIR
              # Point the model-download cache at the symlink tree created in
              # the godot-integration-test derivation. Mirrors docs/conftest.py
              # so the hf_path_test runs offline.
              export XDG_CACHE_HOME=${godot-integration-test}/hf-cache
              ./game --headless
              touch $out
            '';
      in
      {
        # the godot gdextension dynamic lib
        packages.default = nobodywho-godot;

        # checks
        checks.default = flutter_tests;
        checks.flutter_tests = flutter_tests;
        checks.build-godot = nobodywho-godot;
        checks.godot-integration-test = run-godot-integration-test;

        checks.cargo-test = nobodywho-tested;
        checks.nobodywho-python = nobodywho-python;

        checks.react-native-jest = pkgs.buildNpmPackage {
          pname = "react-native-jest";
          version = "0.0.0"; # nix derivation metadata only, does not need to match the npm package version
          src = ./nobodywho/react-native;
          npmDepsHash = "sha256-eGwnlJDmZsPFPg9yHvMniDwi5m2Wq9OAKw/Nh9+MZA0=";
          dontNpmBuild = true;
          checkPhase = "npx jest";
          doCheck = true;
          installPhase = "touch $out";
        };

        # the Everything devshell
        devShells.default = pkgs.callPackage ./nobodywho/shell.nix { inherit android-nixpkgs; };

        # godot stuff
        packages.nobodywho-godot = nobodywho-godot;

        # flutter stuff
        packages.flutter_rust = nobodywho-flutter;

        # python stuff
        packages.nobodywho-python = nobodywho-python;
        devShells.nobodywho-python = pkgs.mkShell {
          # a devshell that includes the built python package
          # useful for testing local changes in repl or pytest
          packages = [
            (nobodywho-python.override { doCheck = false; })
            pkgs.python3Packages.pytest
            pkgs.python3Packages.pytest-asyncio
          ];
        };

        devShells.android = pkgs.callPackage ./nobodywho/android.nix { inherit android-nixpkgs; };
      }
    );
}
