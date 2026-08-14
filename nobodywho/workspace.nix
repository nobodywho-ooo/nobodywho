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
  withLlamaRuntime =
    attrs:
    let
      dependency =
        name: dependencies:
        lib.findFirst (item: item.crateName == name) (throw "${attrs.crateName}: missing ${name}") dependencies;
      core = dependency "nobodywho" attrs.dependencies;
      llama = dependency "llama-cpp-sys-2" core.dependencies;
      runtime = "${llama.lib}/lib/llama-cpp-sys-2.out";
    in
    {
      postInstall = (attrs.postInstall or "") + ''
        # CMake's GNUInstallDirs resolves the libdir per platform: lib64 on some
        # Linux distros, lib elsewhere. llama-cpp-sys-2 emits link-search entries
        # for both and probes the same pair, so neither can be assumed here.
        # Probed with a plain glob rather than `compgen`, which stdenv's shell
        # does not provide. With nullglob off the unmatched pattern stays literal
        # and fails -e; with it on the loop body never runs. Correct either way.
        runtimeLibDir=""
        for candidate in ${runtime}/lib64 ${runtime}/lib; do
          for probe in "$candidate"/libggml*; do
            if [ -e "$probe" ]; then
              runtimeLibDir="$candidate"
              break 2
            fi
          done
        done
        if [ -z "$runtimeLibDir" ]; then
          echo "no llama runtime libraries under ${runtime}/{lib64,lib}" >&2
          ls -la ${runtime} >&2
          exit 1
        fi
        cp -L "$runtimeLibDir"/libggml* "$runtimeLibDir"/libllama* "$lib/lib/"
        cp -L ${runtime}/backends/* "$lib/lib/"
      '' + lib.optionalString pkgs.stdenv.hostPlatform.isDarwin ''
        binding="$lib/lib/lib${attrs.libName}.dylib"
        for path in @loader_path ${pkgs.onnxruntime}/lib; do
          if ! otool -l "$binding" | grep -q "path $path "; then
            install_name_tool -add_rpath "$path" "$binding"
          fi
        done
      '' + lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
        ${pkgs.patchelf}/bin/patchelf --add-rpath '$ORIGIN:${pkgs.onnxruntime}/lib' "$lib/lib/lib${attrs.libName}.so"
      '';
    };

  buildRustCrateForPkgs =
    pkgs:
    pkgs.buildRustCrate.override {
      defaultCrateOverrides = pkgs.defaultCrateOverrides // {
        llama-cpp-sys-2 = attrs: {
          env.LIBCLANG_PATH = "${pkgs.libclang.lib}/lib/libclang.so";

          # Upstream derives `.` as the target dir from crate2nix's shorter OUT_DIR.
          preConfigure = ''
            mkdir -p target/deps
            ln -s target/deps deps
          '';

          # crate2nix does not read the workspace Cargo config.
          env.CMAKE_PROJECT_INCLUDE = "${./core/cmake/llama-build-overrides.cmake}";

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

        nobodywho-flutter = attrs: withLlamaRuntime attrs // {
          env.NOBODYWHO_SKIP_CODEGEN = "True";
          nativeBuildInputs = [
            # this needs to be available at link-time
            vulkan-loader
            pkgs.onnxruntime
            flutter335
          ];
        };

        nobodywho-godot = attrs: lib.optionalAttrs (!pkgs.stdenv.hostPlatform.isAndroid) (withLlamaRuntime attrs) // {
          nativeBuildInputs = [
            # XXX: can we do this with propagatedNativeBuildInputs??
            # this needs to be available at link-time
            vulkan-loader
            pkgs.onnxruntime
          ];
        };

        nobodywho-python = attrs: withLlamaRuntime attrs // {
          nativeBuildInputs = [
            vulkan-loader
            pkgs.onnxruntime
            pkgs.python3
          ];
        };

        nobodywho-uniffi = withLlamaRuntime;
      };
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
    inherit pkgs buildRustCrateForPkgs;
  };
in
cargoNix
