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
            # buildRustCrate's run-tests derivation does NOT inherit nativeBuildInputs
            # from the crate override, so install_name_tool / codesign aren't on PATH
            # by default on macOS. Inject them explicitly. On non-Darwin these vars
            # are no-ops since nothing references @rpath/... in a way ld.so can't
            # resolve via LD_LIBRARY_PATH.
            ${if pkgs.stdenv.isDarwin then ''
              export PATH="${pkgs.darwin.cctools}/bin:${pkgs.darwin.sigtool}/bin:$PATH"
            '' else ""}

            export TEST_MODEL=${test-models.TEST_MODEL}
            export TEST_EMBEDDINGS_MODEL=${test-models.TEST_EMBEDDINGS_MODEL}
            export TEST_CROSSENCODER_MODEL=${test-models.TEST_CROSSENCODER_MODEL}

            # llama-cpp-2 bakes GGML_BACKENDS_DIR via option_env! at compile time. In
            # the Nix sandbox that path is the llama-cpp-sys-2 build dir, which is
            # destroyed when the sys derivation finishes — so the test process finds
            # zero dynamic backends and llama_model_load_from_file returns NULL.
            # The same closure path also has libllama/libggml/libggml-base under
            # lib64/. The macOS test binary references @rpath/libggml-base.0.dylib but
            # has no LC_RPATH entries (crate2nix doesn't add one), so dyld errors with
            # "no LC_RPATH's found". Set DYLD_LIBRARY_PATH (and LD_LIBRARY_PATH on
            # Linux) to point at lib64/, and GGML_BACKEND_DIR at backends/.
            for d in /nix/store/*-rust_llama-cpp-sys-2-*-lib/lib/llama-cpp-sys-2.out; do
              if [ -d "$d/backends" ] && ls "$d"/backends/libggml-* >/dev/null 2>&1; then
                export GGML_BACKEND_DIR="$d/backends"
                # llama-cpp-sys-2's cmake install drops the main shared libs in `lib/`
                # on macOS and `lib64/` on Linux. Use whichever one actually exists.
                libdir=""
                for candidate in "$d/lib" "$d/lib64"; do
                  if [ -d "$candidate" ] && ls "$candidate"/libggml-base.* >/dev/null 2>&1; then
                    libdir="$candidate"
                    break
                  fi
                done
                if [ -n "$libdir" ]; then
                  # Linux ld.so honors LD_LIBRARY_PATH for @rpath-style lookups.
                  export LD_LIBRARY_PATH="$libdir''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
                  # macOS dyld does NOT consult DYLD_LIBRARY_PATH for @rpath/... refs —
                  # @rpath resolves only through the binary's LC_RPATH commands. crate2nix
                  # doesn't add one, so the test binary aborts with "no LC_RPATH's found".
                  # Pre-patch every Mach-O test executable to add libdir as an rpath, then
                  # ad-hoc re-sign on Apple Silicon (modification invalidates the
                  # signature). The binaries live in this sandbox only — we don't ship
                  # patched copies anywhere.
                  if command -v install_name_tool >/dev/null 2>&1; then
                    echo "test-runtime: patching test-binary rpaths in target/debug/ -> $libdir"
                    found=0
                    for bin in target/debug/* target/debug/deps/*; do
                      [ -f "$bin" ] || continue
                      file "$bin" 2>/dev/null | grep -q 'Mach-O.*executable' || continue
                      found=$((found+1))
                      install_name_tool -add_rpath "$libdir" "$bin" 2>&1 || true
                      if command -v codesign >/dev/null 2>&1; then
                        codesign -fs - "$bin" 2>&1 || true
                      fi
                    done
                    echo "test-runtime: patched $found Mach-O test executables"
                  fi
                fi
                echo "test-runtime: GGML_BACKEND_DIR=$GGML_BACKEND_DIR libdir=$libdir"
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
