{
  pkgs,
  lib,
  rustPlatform,
  llvmPackages,
  cmake,
  git,
  vulkan-headers,
  vulkan-loader,
  vulkan-tools,
  shaderc,
  mesa,
  rustfmt,

  # flutter stuff
  flutter335,

  # extra args — unused while using pre-generated Cargo.nix, but kept for easy switch-back
  crate2nix,
  stdenv,
}:

let
  buildRustCrateForPkgs =
    pkgs:
    pkgs.buildRustCrate.override {
      defaultCrateOverrides = pkgs.defaultCrateOverrides // {
        llama-cpp-sys-2 = attrs: {
          env.LIBCLANG_PATH = "${pkgs.libclang.lib}/lib/libclang.so";

          # crate2nix builds llama-cpp-sys-2 as an isolated derivation that never reads
          # the workspace .cargo/config.toml, so re-inject our CMake override here.
          # Without it the dynamic-link ggml/llama libs are born versioned with a
          # build-tree /build/... rpath, which nixpkgs' audit-tmpdir hook rejects
          # ("forbidden reference to /build/"). The override strips the SOVERSION and
          # bakes an $ORIGIN rpath instead.
          env.CMAKE_PROJECT_INCLUDE = "${./scripts/llama-build-overrides.cmake}";

          # Architecture-specific CPU feature flags
          # For ARM64: use defaults for compatibility with weaker devices (Raspberry Pi, etc.)
          env.CARGO_CFG_TARGET_FEATURE =
            if pkgs.stdenv.hostPlatform.isx86_64 then "sse4.2,fma,avx,avx512" else "";

          nativeBuildInputs = [
            llvmPackages.bintools
            cmake
            rustPlatform.bindgenHook
            rustPlatform.cargoBuildHook
            vulkan-headers
            vulkan-loader
            shaderc
            vulkan-tools
            mesa
            git
            rustfmt
          ];
          # TODO: clean up in all these buildinputs
          propagatedBuildInputs = [
            vulkan-loader
            vulkan-headers
            shaderc
            vulkan-tools
            mesa
          ];

        };

        ort-sys = attrs: {
          env.ORT_LIB_PATH = "${pkgs.onnxruntime}/lib";
          env.ORT_PREFER_DYNAMIC_LINK = "1";
          buildInputs = [ pkgs.onnxruntime ];
        };

        # XXX: this is a mildly crazy hack that the clanker came up with in order to
        #      fix nix builds that depend on pyo3. It seems like some environment
        #      variables aren't being passed into the build properly, so we re-set it here
        #
        # The machine-written comment says:
        # pyo3 0.29 moved Python interpreter-config resolution into pyo3-ffi's
        # build script, which exports the config to dependent build scripts over
        # cargo's `links` metadata channel. pyo3's build script then reads it from
        # the `DEP_<links>_PYO3_CONFIG` env var (`DEP_PYTHON_PYO3_CONFIG` for
        # pyo3-ffi, `DEP_PYO3_PYTHON_PYO3_CONFIG` for pyo3 itself).
        #
        # nixpkgs' buildRustCrate derives that env-var prefix from the *crate name*
        # instead of the crate's `links` value (see configure-crate.nix: CRATENAME).
        # pyo3-ffi (links = "python") and pyo3 (links = "pyo3-python") are the rare
        # crates where the two differ, so buildRustCrate emits the config under
        # DEP_PYO3_FFI_* / DEP_PYO3_* and pyo3 never finds it, panicking with
        # "`pyo3_build_config::get()` requires a direct dependency on `pyo3` or
        # `pyo3-ffi`". We bridge the names by appending an alias to the `env` file
        # that each crate installs and its dependents source.
        pyo3-ffi = attrs: {
          postConfigure = ''
            if [ -f target/env ]; then
              echo '[ -n "$DEP_PYO3_FFI_PYO3_CONFIG" ] && export DEP_PYTHON_PYO3_CONFIG="$DEP_PYO3_FFI_PYO3_CONFIG"' >> target/env
            fi
          '';
        };

        pyo3 = attrs: {
          postConfigure = ''
            if [ -f target/env ]; then
              echo '[ -n "$DEP_PYO3_PYO3_CONFIG" ] && export DEP_PYO3_PYTHON_PYO3_CONFIG="$DEP_PYO3_PYO3_CONFIG"' >> target/env
            fi
          '';
        };

        espeak-rs-sys = attrs: {
          nativeBuildInputs = [
            cmake
            rustPlatform.bindgenHook
          ];
          buildInputs = [ pkgs.sonic ];
          env.LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib/libclang.so";
          prePatch = ''
            substituteInPlace build.rs \
              --replace-fail 'get_cargo_target_dir().unwrap()' \
                             'get_cargo_target_dir().unwrap_or_else(|_| out_dir.clone())'
          '';
        };

        nobodywho = attrs: {
          nativeBuildInputs = [
            # this needs to be available at link-time
            vulkan-loader
            pkgs.onnxruntime
          ];
        };

        nobodywho-flutter = attrs: {
          env.NOBODYWHO_SKIP_CODEGEN = "True";
          nativeBuildInputs = [
            # this needs to be available at link-time
            vulkan-loader
            pkgs.onnxruntime
            flutter335
          ];
        };

        nobodywho-godot = attrs: {
          nativeBuildInputs = [
            # XXX: can we do this with propagatedNativeBuildInputs??
            # this needs to be available at link-time
            vulkan-loader
            pkgs.onnxruntime
          ];
        };

        nobodywho-python = attrs: {
          nativeBuildInputs = [
            vulkan-loader
            pkgs.onnxruntime
            pkgs.python3
          ];
        };
      };
    };

  # IFD-based generation — broken with git workspace deps using inheritance (crate2nix#207).
  # To switch back, uncomment crate2nix/stdenv args above and use this instead of Cargo.nix import.
  # generatedCargoNix = crate2nix.tools.${stdenv.hostPlatform.system}.generatedCargoNix {
  #   name = "nobodywho";
  #   src = ./.;
  # };

  # Pre-generated with: nix run github:nix-community/crate2nix -- generate -h crate-hashes.json
  # Re-run when Cargo.toml or Cargo.lock changes.
  cargoNix = import ./Cargo.nix {
    inherit pkgs buildRustCrateForPkgs;
  };
in
cargoNix
