{
  lib,
  flutter,
  callPackage,
  nobodywho_flutter_rust,
}:

let
  models = callPackage ../../models.nix { };
in
flutter.buildFlutterApplication {
  pname = "nobodywho-flutter-tests";
  version = "0.0.1";

  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.difference ./. ./example;
  };

  # Force offline mode to prevent network access attempts
  PUB_OFFLINE = true;
  FLUTTER_OFFLINE = true;
  pubGetFlags = "--offline";

  # No actual app to build - this derivation only runs tests
  buildPhase = ''
    runHook preBuild
    runHook postBuild
  '';

  # Satisfy required outputs without building an app
  installPhase = ''
    runHook preInstall
    touch $out
    touch $debug
    runHook postInstall
  '';

  doCheck = true;
  checkPhase = ''
    runHook preCheck

    echo "Running flutter analyze..."
    flutter analyze --no-fatal-warnings

    echo "Testing the nobodywho dart wrapper package"
    export LD_LIBRARY_PATH="${nobodywho_flutter_rust.lib}/lib"
    export TEST_MODEL="${models.TEST_MODEL}"
    export TEST_EMBEDDINGS_MODEL="${models.TEST_EMBEDDINGS_MODEL}"
    export TEST_CROSSENCODER_MODEL="${models.TEST_CROSSENCODER_MODEL}"
    export TEST_WHISPER_MODEL="${models.TEST_WHISPER_MODEL}"
    export TEST_AUDIO_FILE="${../../../assets/sound.mp3}"
    # Populate the HuggingFace download cache so doctests that hardcode the
    # whisper repo ID resolve offline (the cache dir + .nobodywho-complete
    # marker short-circuit the network call in core/src/huggingface.rs).
    # Nix store paths are read-only, so stage into a writable TMPDIR cache.
    export XDG_CACHE_HOME="$TMPDIR/cache"
    mkdir -p "$XDG_CACHE_HOME/nobodywho/models"
    cp -R ${models.TEST_WHISPER_HF_CACHE}/onnx-community "$XDG_CACHE_HOME/nobodywho/models/"

    flutter test --fail-fast test/

    runHook postCheck
  '';

  autoPubspecLock = ./pubspec.lock;
}
