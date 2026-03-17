{
  doCheck ? true,
  callPackage,
  python3,
  python3Packages,
  workspace, # crate2nix workspace from workspace.nix
}:

let
  models = callPackage ../models.nix { };
  pyprojectToml = builtins.fromTOML (builtins.readFile ./pyproject.toml);
  # Phase 1: the .so is already built by crate2nix via workspace
  nobodywho-python-rs = workspace.workspaceMembers.nobodywho-python.build;
in
python3Packages.buildPythonPackage {
  pname = "nobodywho";
  version = pyprojectToml.project.version;
  format = "other";

  # No Rust source needed — we're just installing the pre-built .so
  dontUnpack = true;

  installPhase = ''
    runHook preInstall
    mkdir -p $out/${python3.sitePackages}
    cp ${nobodywho-python-rs.lib}/lib/libnobodywho_python.so \
       $out/${python3.sitePackages}/nobodywho.abi3.so
    runHook postInstall
  '';

  inherit doCheck;

  nativeCheckInputs = with python3Packages; [
    pytestCheckHook
    pytest-asyncio
    pytest-markdown-docs
  ];

  # Since we used dontUnpack, copy test sources and docs for doctests.
  # pytest config is passed explicitly via flags instead of relying on pyproject.toml.
  # The symlink satisfies docs/conftest.py which resolves test images via
  # Path(__file__).parent.parent / "nobodywho" / "python" / "tests" / "img"
  preCheck = ''
    # The "mkdir -p nobodywho/..." below creates a nobodywho/ directory that Python 3
    # treats as an implicit namespace package, shadowing our real .abi3.so from site-packages.
    # Prepending $out to PYTHONPATH ensures the real module wins import resolution.
    export PYTHONPATH="$out/${python3.sitePackages}:$PYTHONPATH"
    cp -r ${../python/tests} tests
    cp -r ${../../docs} docs
    mkdir -p nobodywho/python/tests
    ln -s ../../../tests/img nobodywho/python/tests/img
  '';

  pytestFlagsArray = [
    "tests"
    "docs"
    "-o=python_files=test_*.py *.md"
    "-o=confcutdir=."
  ];

  # Vision/multimodal tests are too slow in the nix sandbox (no GPU access)
  disabledTestPaths = [
    "tests/test_multimodal.py"
  ];

  env.TEST_MODEL = models.TEST_MODEL;
  env.TEST_EMBEDDINGS_MODEL = models.TEST_EMBEDDINGS_MODEL;
  env.TEST_CROSSENCODER_MODEL = models.TEST_CROSSENCODER_MODEL;
  # not needed since we skip vision tests
  # env.TEST_VISION_MODEL = models.TEST_VISION_MODEL;
  # env.TEST_MMPROJ_MODEL = models.TEST_MMPROJ_MODEL;
  # TODO: reintroduce vision tests when we can make them fast
}
