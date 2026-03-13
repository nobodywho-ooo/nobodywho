{
  doCheck ? true,
  callPackage,
  python3Packages,
  rustPlatform,
  cmake,
  rustfmt,
  vulkan-headers,
  vulkan-loader,
  shaderc,
  vulkan-tools,
}:

let
  models = callPackage ../models.nix { };
in
python3Packages.buildPythonPackage {
  pname = "nobodywho";
  version = "0.0.0";
  pyproject = true;

  src = ../.;
  buildAndTestSubdir = "python";

  cargoDeps = rustPlatform.importCargoLock {
    lockFile = ../Cargo.lock;
    outputHashes = {
      "llama-cpp-2-0.1.139" = "sha256-ZOPzf1uziCzPZHni1RsWzQiKH3NX9HAfmKF9P1jrbnk=";
    };
  };

  nativeBuildInputs = [
    rustfmt
    rustPlatform.bindgenHook
    rustPlatform.cargoSetupHook
    rustPlatform.maturinBuildHook
    cmake

    # vulkan stuff
    shaderc
    vulkan-headers
    vulkan-loader
    vulkan-tools
  ];

  buildInputs = [
    shaderc
    vulkan-headers
    vulkan-loader
    vulkan-tools
  ];

  dontUseCmakeConfigure = true;

  inherit doCheck;

  nativeCheckInputs = with python3Packages; [
    pytestCheckHook
    pytest
    pytest-asyncio
  ];

  env.TEST_MODEL = models.TEST_MODEL;
  env.TEST_EMBEDDINGS_MODEL = models.TEST_EMBEDDINGS_MODEL;
  env.TEST_CROSSENCODER_MODEL = models.TEST_CROSSENCODER_MODEL;
  env.TEST_VISION_MODEL = models.TEST_VISION_MODEL;
  env.TEST_MMPROJ_MODEL = models.TEST_MMPROJ_MODEL;
}
