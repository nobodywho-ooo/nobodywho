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
      "gbnf-0.2.5" = "sha256-X0tRU70M7U7xatUCr+LiMP5hHh4aeVjPe8RfDpvt9W4=";
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
}
