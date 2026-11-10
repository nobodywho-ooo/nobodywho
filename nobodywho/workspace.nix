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

  # If set, build for wasm32-unknown-emscripten instead of for the host, with
  # this Rust toolchain (which must have the std for that target).
  emscriptenRustToolchain ? null,
  emscripten,
  python3,
  unzip,
  fetchurl,
}:

let
  forEmscripten = emscriptenRustToolchain != null;

  # nixpkgs can't cross-compile to Emscripten, so give Cargo.nix and
  # buildRustCrate a made-up platform for it. They only use it for `--target`,
  # the `CARGO_CFG_*` variables and `cfg(...)` dependencies; C/C++ code is
  # compiled by Emscripten (see `emscriptenCrateOverrides`).
  emscriptenPkgs = pkgs // {
    stdenv = pkgs.stdenv // {
      hostPlatform = lib.systems.elaborate {
        config = "wasm32-unknown-none";
        isUnix = true;
        rust = {
          rustcTarget = "wasm32-unknown-emscripten";
          rustcTargetSpec = "wasm32-unknown-emscripten";
          platform = {
            arch = "wasm32";
            os = "emscripten";
            env = "";
            vendor = "unknown";
            target-family = [
              "unix"
              "wasm"
            ];
          };
        };
      };
    };
    # For build scripts and proc-macros.
    buildPackages = pkgs;
  };

  # `--use-port=emdawnwebgpu` (in js/rust/build.rs and llama.cpp's CMake) makes
  # Emscripten download Dawn's WebGPU port, which the sandbox doesn't allow.
  # Must match `tools/ports/emdawnwebgpu.py` in nixpkgs' Emscripten.
  # Disabled for now, since WebGPU support is disabled
  # (see js/rust/build.rs and core/Cargo.toml).
  # emdawnwebgpuVersion = "v20260423.175430";
  # emdawnwebgpuUrl = "https://github.com/google/dawn/releases/download/${emdawnwebgpuVersion}/emdawnwebgpu_pkg-${emdawnwebgpuVersion}.zip";
  # emdawnwebgpu = fetchurl {
  #   url = emdawnwebgpuUrl;
  #   sha512 = "42784f70b67197c614322f9fabb0f1dc64228a0de10e88f99941fa9d29bee9ad6683f4651d4eefd5a7a9fbd1f976eb522b190b683219ed1793e9b531c602ffa6";
  # };

  # For crates that run Emscripten.
  emscriptenSetup = {
    nativeBuildInputs = [ emscripten ];
    env = {
      # stdenv sets `CC` etc. to the host compiler, which cc-rs would use.
      CC_wasm32_unknown_emscripten = "emcc";
      CXX_wasm32_unknown_emscripten = "em++";
      AR_wasm32_unknown_emscripten = "emar";
      # Same as js/rust/build.sh.
      CFLAGS_wasm32_unknown_emscripten = "-fPIC";
      CXXFLAGS_wasm32_unknown_emscripten = "-fPIC";
    };
    preConfigure = ''
      # Emscripten builds system libraries and ports into its cache on
      # demand, so it must be writable.
      export EM_CACHE=$TMPDIR/emscripten-cache
      cp -r ${emscripten}/share/emscripten/cache $EM_CACHE
      chmod -R u+w $EM_CACHE

      # Re-enable together with `emdawnwebgpu` above (and `--use-port=emdawnwebgpu`
      # in js/rust/build.rs) when WebGPU support comes back.
      # if ! grep -qF "''${emdawnwebgpuVersion}" ${emscripten}/share/emscripten/tools/ports/emdawnwebgpu.py; then
      #   echo "ERROR: Update emdawnwebgpu in workspace.nix to what Emscripten wants:"
      #   grep -E '^(_VERSION|SHA512) =' ${emscripten}/share/emscripten/tools/ports/emdawnwebgpu.py
      #   exit 1
      # fi
      # Where Emscripten's `Ports.fetch_port_artifact` would unpack it.
      # mkdir -p $EM_CACHE/ports/emdawnwebgpu
      # ${unzip}/bin/unzip -q ''${emdawnwebgpu} -d $EM_CACHE/ports/emdawnwebgpu
      # echo ''${emdawnwebgpuUrl} > $EM_CACHE/ports/emdawnwebgpu/.emscripten_url
    '';
    # The host's strip doesn't know WASM.
    dontStrip = true;
  };

  emscriptenCrateOverrides = {
    llama-cpp-sys-2 =
      attrs:
      emscriptenSetup
      // {
        nativeBuildInputs = emscriptenSetup.nativeBuildInputs ++ [
          cmake
          # llama.cpp's WebGPU backend embeds its shaders with a Python script.
          python3
        ];
        # Not `rustPlatform.bindgenHook`, it passes the host's headers.
        env = emscriptenSetup.env // {
          LIBCLANG_PATH = "${emscripten.llvmEnv}/lib";
        };
      };

    onig_sys = attrs: emscriptenSetup;

    # No ONNX Runtime or Vulkan on Emscripten.
    ort-sys = attrs: { };
    nobodywho = attrs: { };

    # Only needed as a library of `nobodywho-js`, don't link its cdylib,
    # staticlib and `uniffi-bindgen` binary.
    nobodywho-uniffi = attrs: {
      type = [ "lib" ];
      crateBin = [ ];
    };

    # Builds `uniffi.{js,wasm}`, like `js/rust/build.sh`.
    nobodywho-js =
      attrs:
      emscriptenSetup
  };

  # The build scripts and proc-macros must be built by the same rustc too.
  toolchainOverrides = lib.optionalAttrs forEmscripten {
    rustc = emscriptenRustToolchain;
    cargo = emscriptenRustToolchain;
  };

  buildRustCrateForPkgs =
    pkgs:
    let
      onnxruntime = pkgs.callPackage ./onnxruntime.nix { };
      isEmscripten = pkgs.stdenv.hostPlatform.rust.rustcTarget == "wasm32-unknown-emscripten";
    in
    (pkgs.buildRustCrate.override (
      toolchainOverrides // lib.optionalAttrs isEmscripten { inherit (pkgs) stdenv; }
    )).override
    {
      defaultCrateOverrides = pkgs.defaultCrateOverrides // {
        llama-cpp-sys-2 = attrs: {
          env.LIBCLANG_PATH = "${pkgs.libclang.lib}/lib/libclang.so";

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
          # Static link, as in release builds.
          env.ORT_LIB_PATH = "${onnxruntime}/lib";
          buildInputs = [ onnxruntime ];
          # ort-sys's `copy-dylibs` feature symlinks onnxruntime libs into
          # OUT_DIR.ancestors(3)/{examples,deps}. buildRustCrate sets
          # OUT_DIR=$(pwd)/target/build/ort-sys.out, so ancestors(3) is the
          # source root — but `examples` and `deps` don't exist there.
          preConfigure = ''
            mkdir -p examples deps
          '';
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
          # Stands in for the removed `pyo3/extension-module` cargo feature, which
          # maturin now sets through this variable instead: without it pyo3 links
          # libpython and demands an interpreter that isn't in the nix sandbox.
          env.PYO3_BUILD_EXTENSION_MODULE = "1";

          postConfigure = ''
            if [ -f target/env ]; then
              echo '[ -n "$DEP_PYO3_FFI_PYO3_CONFIG" ] && export DEP_PYTHON_PYO3_CONFIG="$DEP_PYO3_FFI_PYO3_CONFIG"' >> target/env
            fi
          '';
        };

        pyo3 = attrs: {
          env.PYO3_BUILD_EXTENSION_MODULE = "1";

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
            onnxruntime
          ];
        };

        nobodywho-flutter = attrs: {
          env.NOBODYWHO_SKIP_CODEGEN = "1";
          nativeBuildInputs = [
            # this needs to be available at link-time
            vulkan-loader
            onnxruntime
            flutter335
          ];
        };

        nobodywho-godot = attrs: {
          nativeBuildInputs = [
            # XXX: can we do this with propagatedNativeBuildInputs??
            # this needs to be available at link-time
            vulkan-loader
            onnxruntime
          ];
        };

        nobodywho-python = attrs: {
          nativeBuildInputs = [
            vulkan-loader
            onnxruntime
            pkgs.python3
          ];
        };
      }
      // lib.optionalAttrs isEmscripten emscriptenCrateOverrides;
    };

  # IFD-based generation — broken with git workspace deps using inheritance (crate2nix#207).
  # To switch back, uncomment crate2nix/stdenv args above and use this instead of Cargo.nix import.
  # generatedCargoNix = crate2nix.tools.${stdenv.hostPlatform.system}.generatedCargoNix {
  #   name = "nobodywho";
  #   src = ./.;
  # };

  # ── Regenerating Cargo.nix ──────────────────────────────────────────────
  # Cargo.nix is checked in and consumed directly (instead of generated via
  # import-from-derivation) because crate2nix's IFD path is broken for git
  # workspace deps that use inheritance (crate2nix#207).
  #
  # Regenerate whenever Cargo.toml or Cargo.lock changes:
  #
  #   cd nobodywho
  #   nix run github:nix-community/crate2nix -- generate -h crate-hashes.json
  #
  # The -h crate-hashes.json pins hashes for git/path deps so the generated
  # Cargo.nix is reproducible. Review the diff and commit Cargo.nix alongside
  # the Cargo.toml/Cargo.lock changes.
  # ─────────────────────────────────────────────────────────────────────────
  cargoNix = import ./Cargo.nix {
    inherit buildRustCrateForPkgs;
    pkgs = if forEmscripten then emscriptenPkgs else pkgs;
  };
in
cargoNix
