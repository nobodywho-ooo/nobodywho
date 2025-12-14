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

          # this is a bit of a hack- should not be supplied like this
          env.CARGO_CFG_TARGET_FEATURE = "sse4.2,fma,avx,avx512";
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
            flutter335
            rustfmt
          ];
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
          ];
        };
      };
    };

  generatedCargoNix = crate2nix.tools.${stdenv.hostPlatform.system}.generatedCargoNix {
    name = "nobodywho-flutter";
    src = ../../.;
  };

  cargoNix = import generatedCargoNix {
    inherit pkgs buildRustCrateForPkgs;
  };
in
cargoNix.workspaceMembers.nobodywho-flutter.build
# .override {
#   env.NOBODYWHO_SKIP_CODEGEN = "True";
# }

# rustPlatform.buildRustPackage {
#   pname = "nobodywho-flutter-rust";
#   version = "0.0.0";
#   src = lib.cleanSourceWith {
#     src = ../../.;
#     filter =
#       path: type:
#       let
#         # Get the relative path from the source root (which is nobodywho/)
#         relPath = lib.removePrefix (toString ../../.) (toString path);
#         # Check if this is a directory we need (or parent of one we need)
#         isRelevantDir =
#           relPath == ""
#           # root dir
#           || relPath == "/flutter"
#           # flutter dir itself
#           || lib.hasPrefix "/core" relPath
#           || lib.hasPrefix "/flutter/rust" relPath
#           || lib.hasPrefix "/flutter/nobodywho_flutter" relPath;
#         # Check if it's a relevant file in a relevant directory
#         isRelevantFile =
#           (
#             lib.hasPrefix "/core" relPath
#             || lib.hasPrefix "/flutter/rust" relPath
#             || lib.hasPrefix "/flutter/nobodywho_flutter" relPath
#             || relPath == "/Cargo.lock"
#             || relPath == "/Cargo.toml"
#           )
#           && lib.any (ext: lib.hasSuffix ext path) [
#             ".rs"
#             ".toml"
#             "Cargo.lock"
#           ];
#       in
#       (type == "directory" && isRelevantDir) || (type == "regular" && isRelevantFile);
#   };
#   env.NOBODYWHO_SKIP_CODEGEN = "True";
#   buildAndTestSubdir = "flutter/rust";
#
#   # Patch the workspace to only include members we have in the filtered source
#   postPatch = ''
#         substituteInPlace Cargo.toml \
#           --replace-fail 'members = [
#         "core",
#         "godot",
#         "unity",
#         "flutter/rust",
#         "python",
#     ]' 'members = [
#         "core",
#         "flutter/rust"
#     ]'
#   '';
#
#   nativeBuildInputs = [
#     llvmPackages.bintools
#     cmake
#     rustPlatform.bindgenHook
#     rustPlatform.cargoBuildHook
#     vulkan-headers
#     vulkan-loader
#     shaderc
#     vulkan-tools
#     mesa
#     git
#     flutter335
#   ];
#   buildInputs = [
#     vulkan-loader
#     vulkan-headers
#     shaderc
#     vulkan-tools
#     mesa
#   ];
#   cargoLock = {
#     lockFile = ../../Cargo.lock;
#     outputHashes = { };
#   };
#   doCheck = false;
# }
