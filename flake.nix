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
  };
  outputs =
    {
      nixpkgs,
      flake-utils,
      android-nixpkgs,
      crate2nix,
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
          }
        );

        workspace = pkgs.callPackage ./nobodywho/workspace.nix { inherit crate2nix; };

        # python stuff
        nobodywho-python = pkgs.callPackage ./nobodywho/python { inherit workspace; };

        # flutter stuff
        nobodywho-flutter = workspace.workspaceMembers.nobodywho-flutter.build;
        flutter_tests = pkgs.callPackage ./nobodywho/flutter/nobodywho {
          nobodywho_flutter_rust = nobodywho-flutter;
          inherit nobodywho-stt;
        };

        # cargo tests
        test-models = pkgs.callPackage ./nobodywho/models.nix { };
        nobodywho-tested = workspace.workspaceMembers.nobodywho.build.override {
          runTests = true;
          testPreRun = ''
            export TEST_MODEL=${test-models.TEST_MODEL}
            export TEST_EMBEDDINGS_MODEL=${test-models.TEST_EMBEDDINGS_MODEL}
            export TEST_CROSSENCODER_MODEL=${test-models.TEST_CROSSENCODER_MODEL}

            # llama-cpp-2 bakes GGML_BACKENDS_DIR via option_env! at compile time. In
            # Nix that compile-time path is the llama-cpp-sys-2 sandbox build dir, which
            # is destroyed once the sys derivation finishes. Without this override the
            # test process finds zero dynamic backends, llama_model_load_from_file
            # returns NULL, and every model-loading test panics with "null result from
            # llama cpp". Find the installed backends in the closure and point at them.
            #
            # Same store path also has the main shared libs (libllama, libggml,
            # libggml-base) under lib64/. The test binary on macOS was linked with
            # @rpath/libggml-base.dylib install names but no LC_RPATH was added by
            # crate2nix's link step, so dyld fails to find them. Export
            # DYLD_LIBRARY_PATH (linux: LD_LIBRARY_PATH) so dyld/ld.so resolves at
            # runtime.
            for d in /nix/store/*-rust_llama-cpp-sys-2-*-lib/lib/llama-cpp-sys-2.out; do
              if [ -d "$d/backends" ] && ls "$d"/backends/libggml-* >/dev/null 2>&1; then
                export GGML_BACKEND_DIR="$d/backends"
                if [ -d "$d/lib64" ]; then
                  export DYLD_LIBRARY_PATH="$d/lib64''${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
                  export DYLD_FALLBACK_LIBRARY_PATH="$d/lib64''${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}"
                  export LD_LIBRARY_PATH="$d/lib64''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
                fi
                echo "test-runtime: GGML_BACKEND_DIR=$GGML_BACKEND_DIR"
                break
              fi
            done
          '';
        };

        # STT module
        nobodywho-stt = workspace.workspaceMembers.nobodywho-stt.build;

        # godot stuff
        nobodywho-godot = workspace.workspaceMembers.nobodywho-godot.build;
        godot-integration-test = pkgs.callPackage ./nobodywho/godot/integration-test {
          inherit nobodywho-godot nobodywho-stt;
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
          npmDepsHash = "sha256-CJ6r3KQMDNOpVx4uvNz/jWzmipJVRF9IpqE0gkg1c9o=";
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
        packages.nobodywho-stt = nobodywho-stt;
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
