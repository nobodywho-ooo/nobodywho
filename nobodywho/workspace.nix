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

          # llama-cpp-sys-2's build.rs hard-links the just-built .dylib/.so files into
          # `target_dir/deps/` (computed as 3 ancestors up from OUT_DIR) so that cargo
          # tests in downstream crates can resolve them at runtime. In a normal cargo
          # invocation, `target/<profile>/deps/` already exists. In the crate2nix Nix
          # sandbox on macOS the directory hasn't been created when the build script
          # runs, and `std::fs::hard_link` panics with NotFound. Patch the .unwrap()
          # calls into create_dir_all + ignore-on-error so the build script no longer
          # depends on cargo's directory layout being pre-populated.
          postPatch = (attrs.postPatch or "") + ''
            # The unpacked source root is the entire llama-cpp-rs workspace; the build.rs
            # we want lives in the llama-cpp-sys-2/ subdir.
            substituteInPlace llama-cpp-sys-2/build.rs \
              --replace-fail 'std::fs::hard_link(asset.clone(), dst).unwrap();' \
                'if let Some(p) = dst.parent() { let _ = std::fs::create_dir_all(p); } let _ = std::fs::hard_link(asset.clone(), dst);'
          '';

          # crate2nix preserves the entire cmake OUT_DIR in $out, including the transient
          # build/ subdir. That subdir contains shadow copies of every .so produced by
          # cmake (libllama, libggml, libggml-cpu-*, libggml-vulkan, …) whose RPATHs
          # still point at /build/llama-cpp-rs-…/build, which Nix's fixup phase rejects
          # via the "disallowedReferences" check on /build. The installed copies live in
          # $out/lib/llama-cpp-sys-2.out/{backends,lib64,…} with proper RPATHs and are
          # what the consuming crates link against — the build/ tree is dead weight.
          # Drop it before fixupPhase runs.
          preFixup = ''
            for d in $out $lib; do
              if [ -d "$d/lib/llama-cpp-sys-2.out/build" ]; then
                rm -rf "$d/lib/llama-cpp-sys-2.out/build"
              fi
            done
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
