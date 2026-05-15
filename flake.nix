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
        };

        # cargo tests
        test-models = pkgs.callPackage ./nobodywho/models.nix { };
        nobodywho-tested = workspace.workspaceMembers.nobodywho.build.override {
          runTests = true;
          testPreRun = ''
            ${if pkgs.stdenv.isDarwin then ''
              export PATH="${pkgs.darwin.cctools}/bin:${pkgs.darwin.sigtool}/bin:$PATH"
            '' else ""}

            export TEST_MODEL=${test-models.TEST_MODEL}
            export TEST_EMBEDDINGS_MODEL=${test-models.TEST_EMBEDDINGS_MODEL}
            export TEST_CROSSENCODER_MODEL=${test-models.TEST_CROSSENCODER_MODEL}

            # llama-cpp-2 bakes GGML_BACKENDS_DIR via option_env! at compile time. In the
            # Nix sandbox that path is the llama-cpp-sys-2 build dir, which is destroyed
            # when the sys derivation finishes — the test process finds zero dynamic backends
            # and llama_model_load_from_file returns NULL. Find the installed backends dir
            # from the Nix store and point GGML_BACKEND_DIR at it. On macOS we also need to
            # patch the test binary's LC_RPATH so dyld can resolve @rpath/libggml-base.dylib.
            for d in /nix/store/*-rust_llama-cpp-sys-2-*-lib/lib/llama-cpp-sys-2.out; do
              if [ -d "$d/backends" ] && ls "$d"/backends/libggml-* >/dev/null 2>&1; then
                export GGML_BACKEND_DIR="$d/backends"
                libdir=""
                for candidate in "$d/lib" "$d/lib64"; do
                  if [ -d "$candidate" ] && ls "$candidate"/libggml-base.* >/dev/null 2>&1; then
                    libdir="$candidate"
                    break
                  fi
                done
                if [ -n "$libdir" ]; then
                  export LD_LIBRARY_PATH="$libdir''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
                  if command -v install_name_tool >/dev/null 2>&1; then
                    for bin in target/debug/* target/debug/deps/*; do
                      [ -f "$bin" ] || continue
                      file "$bin" 2>/dev/null | grep -q 'Mach-O.*executable' || continue
                      install_name_tool -add_rpath "$libdir" "$bin" 2>&1 || true
                      if command -v codesign >/dev/null 2>&1; then
                        codesign -fs - "$bin" 2>&1 || true
                      fi
                    done
                  fi
                fi
                break
              fi
            done
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
