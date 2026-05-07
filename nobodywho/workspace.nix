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
          # runs (Linux's `target/` layout differs slightly), and `std::fs::hard_link`
          # panics with NotFound at build.rs:1126. Patch the .unwrap() calls to first
          # create the parent dir and then ignore link errors so the build script no
          # longer depends on cargo's directory layout being pre-populated.
          postPatch = (attrs.postPatch or "") + ''
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

        };

        nobodywho-stt = attrs: {
          nativeBuildInputs = [
            vulkan-loader
          ];
        };

        nobodywho = attrs: {
          nativeBuildInputs = [
            # this needs to be available at link-time
            vulkan-loader
          ] ++ lib.optionals pkgs.stdenv.isDarwin [
            # The test phase of cargo-test runs install_name_tool + codesign in
            # testPreRun (see flake.nix) to add an LC_RPATH to the Mach-O test
            # binary so dyld can resolve @rpath/libggml-base.dylib at runtime.
            # Without these on PATH the patch is skipped silently and tests abort.
            pkgs.darwin.cctools
            pkgs.darwin.sigtool
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
