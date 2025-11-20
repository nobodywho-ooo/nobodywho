{
  python3Packages,
  rustPlatform,
  cmake,
  rustfmt,
  vulkan-headers,
  vulkan-loader,
  shaderc,
  vulkan-tools,
}:

python3Packages.buildPythonPackage {
  pname = "nobodywho";
  version = "0.0.0";
  pyproject = true;

  src = ../.;

  cargoDeps = rustPlatform.importCargoLock { lockFile = ../Cargo.lock; };

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

  buildAndTestSubdir = "python";
}
