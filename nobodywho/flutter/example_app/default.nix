{
  lib,
  flutter335,
  stdenv,
  callPackage,
}:

let
  models = callPackage ../../models.nix { };
  nobodywho_flutter_rust = callPackage ../rust { };
in
flutter335.buildFlutterApplication rec {
  pname = "nobodywho_flutter_test_app";
  version = "0.0.1";

  src = lib.fileset.toSource {
    root = ../.;
    fileset = ../.;
  };

  sourceRoot = "${src.name}/example_app";

  env.NOBODYWHO_FLUTTER_LIB_PATH = "${nobodywho_flutter_rust}/lib/libnobodywho_flutter.so";

  # Force offline mode to prevent network access attempts
  PUB_OFFLINE = true;
  FLUTTER_OFFLINE = true;

  # Additional dart/flutter build flags
  pubGetFlags = "--offline";

  # Fix the plugin symlink for the Nix build environment
  preBuild = ''
    # Set environment variables for offline mode
    export PUB_OFFLINE=true
    export FLUTTER_OFFLINE=true
    export DART_VM_OPTIONS="--no-analytics"

    # Remove the absolute symlink if it exists
    rm -f linux/flutter/ephemeral/.plugin_symlinks/nobodywho_flutter

    # Create a relative symlink to the plugin
    mkdir -p linux/flutter/ephemeral/.plugin_symlinks
    ln -sf ../../../../../nobodywho_flutter linux/flutter/ephemeral/.plugin_symlinks/nobodywho_flutter
  '';

  fixupPhase = ''
    patchelf --add-rpath '$ORIGIN' $out/app/nobodywho_flutter_test_app/lib/libflutter_linux_gtk.so 
  '';

  # read pubspec using IFD
  # (can't be upstreamed to nixpkgs)
  autoPubspecLock = ./pubspec.lock;

  doCheck = true;
  checkPhase = ''
    export PUB_OFFLINE=true
    export FLUTTER_OFFLINE=true
    export DART_VM_OPTIONS="--no-analytics"
    export LD_LIBRARY_PATH="${nobodywho_flutter_rust}/lib"
    export TEST_MODEL="${models.TEST_MODEL}"
    flutter test ../nobodywho_dart/test/nobodywho_dart_test.dart
  '';
}
