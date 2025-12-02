{
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

  # flutter stuff
  flutter335,
}:

rustPlatform.buildRustPackage {
  pname = "nobodywho-flutter-rust";
  version = "0.0.0";
  src = lib.cleanSourceWith {
    src = ../../.;
    filter =
      path: type:
      let
        # Get the relative path from the source root (which is nobodywho/)
        relPath = lib.removePrefix (toString ../../.) (toString path);
        # Check if this is a directory we need (or parent of one we need)
        isRelevantDir =
          relPath == ""
          # root dir
          || relPath == "/flutter"
          # flutter dir itself
          || lib.hasPrefix "/core" relPath
          || lib.hasPrefix "/flutter/rust" relPath
          || lib.hasPrefix "/flutter/nobodywho_flutter" relPath;
        # Check if it's a relevant file in a relevant directory
        isRelevantFile =
          (
            lib.hasPrefix "/core" relPath
            || lib.hasPrefix "/flutter/rust" relPath
            || lib.hasPrefix "/flutter/nobodywho_flutter" relPath
            || relPath == "/Cargo.lock"
            || relPath == "/Cargo.toml"
          )
          && lib.any (ext: lib.hasSuffix ext path) [
            ".rs"
            ".toml"
            "Cargo.lock"
          ];
      in
      (type == "directory" && isRelevantDir) || (type == "regular" && isRelevantFile);
  };
  env.NOBODYWHO_SKIP_CODEGEN = "True";
  buildAndTestSubdir = "flutter/rust";

  # Patch the workspace to only include members we have in the filtered source
  postPatch = ''
        substituteInPlace Cargo.toml \
          --replace-fail 'members = [
        "core",
        "godot",
        "unity",
        "flutter/rust",
        "python",
    ]' 'members = [
        "core",
        "flutter/rust"
    ]'
  '';

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
  ];
  buildInputs = [
    vulkan-loader
    vulkan-headers
    shaderc
    vulkan-tools
    mesa
  ];
  cargoLock = {
    lockFile = ../../Cargo.lock;
  };
  doCheck = false;
}
