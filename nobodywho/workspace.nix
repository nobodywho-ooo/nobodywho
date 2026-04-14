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

        # whisper-rs build.rs reads DEP_WHISPER_WHISPER_CPP_VERSION (set by whisper-rs-sys
        # via `cargo:WHISPER_CPP_VERSION=…`). crate2nix builds each crate in isolation so
        # the output isn't propagated automatically — hard-code it here.
        whisper-rs = attrs: {
          env.DEP_WHISPER_WHISPER_CPP_VERSION = "1.8.3";
        };

        whisper-rs-sys = attrs: {
          env.LIBCLANG_PATH = "${pkgs.libclang.lib}/lib/libclang.so";

          nativeBuildInputs = [
            llvmPackages.bintools
            cmake
            rustPlatform.bindgenHook
            vulkan-headers
            vulkan-loader
            shaderc
            vulkan-tools
            mesa
            git
          ];
          propagatedBuildInputs = [
            vulkan-loader
            vulkan-headers
            shaderc
            vulkan-tools
            mesa
          ];

          # HACK: Both whisper-rs-sys and llama-cpp-sys-2 bundle ggml, producing
          # identically-named static archives and object files. buildRustCrate
          # packs static library objects directly into the .rlib, so both rlibs
          # end up containing ggml object files — causing duplicate symbol errors
          # at the final link step.
          # Fix: strip ggml objects from whisper's .rlib and delete whisper's
          # libggml*.a archives. Whisper's remaining ggml symbol references
          # (in libwhisper.a) resolve against llama's ggml at link time.
          # The two ggml versions are ABI-compatible (additive changes only,
          # same GGML_FILE_VERSION=2 and GGML_QNT_VERSION=2).
          postInstall = ''
            for rlib in $lib/lib/*.rlib; do
              [ -f "$rlib" ] || continue
              objs=$(ar t "$rlib" 2>/dev/null | grep -E '^ggml' || true)
              if [ -n "$objs" ]; then
                echo "Stripping ggml objects from $rlib:"
                echo "$objs"
                echo "$objs" | xargs ar d "$rlib"
              fi
            done
            find $lib/lib -name 'libggml*.a' -print -delete || true
          '';
        };

        nobodywho = attrs: {
          nativeBuildInputs = [
            # this needs to be available at link-time
            vulkan-loader
          ];
        };

        nobodywho-flutter = attrs: {
          env.NOBODYWHO_SKIP_CODEGEN = "True";
          nativeBuildInputs = [
            # this needs to be available at link-time
            vulkan-loader
            flutter335
          ];
        };

        nobodywho-godot = attrs: {
          nativeBuildInputs = [
            # XXX: can we do this with propagatedNativeBuildInputs??
            # this needs to be available at link-time
            vulkan-loader
          ];
        };

        nobodywho-python = attrs: {
          nativeBuildInputs = [
            vulkan-loader
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
