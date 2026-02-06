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

  # extra args
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
          env.CARGO_CFG_TARGET_FEATURE =
            if pkgs.stdenv.hostPlatform.isx86_64 then
              "sse4.2,fma,avx,avx512"
            else if pkgs.stdenv.hostPlatform.isAarch64 then
              "neon,fp16,crypto"
            else
              "";

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
      };
    };

  generatedCargoNix = crate2nix.tools.${stdenv.hostPlatform.system}.generatedCargoNix {
    name = "nobodywho";
    src = ./.;
  };

  cargoNix = import generatedCargoNix {
    inherit pkgs buildRustCrateForPkgs;
  };
in
cargoNix
