{ craneLib, fetchurl, rustPlatform, llvmPackages_12, cmake, vulkan-headers, vulkan-loader, vulkan-tools, shaderc, mesa }:


let
  # Common derivation arguments used for all builds
  commonArgs = {
    src = craneLib.cleanCargoSource ./.;
    strictDeps = true;

    buildInputs = [
      vulkan-loader
      vulkan-headers
      shaderc
      vulkan-tools
      mesa.drivers
    ];

    nativeBuildInputs = [
      llvmPackages_12.bintools
      cmake
      rustPlatform.bindgenHook
      vulkan-headers
      vulkan-loader
      shaderc
      vulkan-tools
      mesa.drivers
    ];

    doNotLinkInheritedArtifacts = true;
    doInstallCargoArtifacts = 1;
  };
  # Build *just* the cargo dependencies, so we can reuse
  # all of that work (e.g. via cachix) when running in CI
  cargoArtifacts = craneLib.buildDepsOnly (commonArgs // {
    # Additional arguments specific to this derivation can be added here.
    # Be warned that using `//` will not do a deep copy of nested
    # structures
    pname = "nobodywho-deps";
  });

  # Run clippy (and deny all warnings) on the crate source,
  # reusing the dependency artifacts (e.g. from build scripts or
  # proc-macros) from above.
  #
  # Note that this is done as a separate derivation so it
  # does not impact building just the crate by itself.
  # nobodywhoClippy = craneLib.cargoClippy (commonArgs // {
  #   # Again we apply some extra arguments only to this derivation
  #   # and not every where else. In this case we add some clippy flags
  #   inherit cargoArtifacts;
  #   cargoClippyExtraArgs = "--all-targets -- --deny warnings";
  # });


  # Build the actual crate itself, reusing the dependency
  # artifacts from above.
  nobodywho = craneLib.buildPackage (commonArgs // {
    inherit cargoArtifacts;
  });

  # Also run the crate tests under cargo-tarpaulin so that we can keep
  # track of code coverage
  # myCrateCoverage = craneLib.cargoTarpaulin (commonArgs // {
  #   inherit cargoArtifacts;
  # });

  oldDrv = rustPlatform.buildRustPackage (commonArgs // {
    pname = "nobodywho";
    version = "0.0.0";
    cargoLock = {
      lockFile = ./Cargo.lock;
      outputHashes = {
        "gdextension-api-0.2.1" = "sha256-YkMbzObJGnmQa1XGT4ApRrfqAeOz7CktJrhYks8z0RY=";
        "godot-0.2.2" = "sha256-6q7BcQ/6WvzJdVmyAVGPMtuIDJFYKaRrkv3/JQBi11M=";
        "llama-cpp-2-0.1.86" = "sha256-Fe8WPO1NAISGGDkX5UWM8ubekYbnnAwEcKf0De5x9AQ=";
      };
    };
    env.TEST_MODEL = fetchurl {
      name = "gemma-2-2b-it-Q4_K_M.gguf";
      url = "https://huggingface.co/bartowski/gemma-2-2b-it-GGUF/resolve/main/gemma-2-2b-it-Q4_K_M.gguf";
      hash = "sha256-4K7oUGDxaPDy2Ec9fqQc4vMjDBvBN0hHUF6lmSiKd4c=";
    };
    env.TEST_EMBEDDINGS_MODEL = fetchurl {
      name = "bge-small-en-v1.5-q8_0.gguf";
      url = "https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/main/bge-small-en-v1.5-q8_0.gguf";
      sha256 = "sha256-7Djo2hQllrqpExJK5QVQ3ihLaRa/WVd+8vDLlmDC9RQ=";
    };

    checkPhase = ''
      cargo test -- --test-threads=1 --nocapture
    '';
    doCheck = true;
  });
in
  nobodywho
