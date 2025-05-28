{ writeShellScriptBin, lib, unityhub, mkShell, ... }:

let
  version = "6000.0.47f1";

  # Reuse the unityhub FHSEnv but with our custom runScript
  unity-editor = writeShellScriptBin "unity-editor" ''
    ${lib.getExe unityhub.fhsEnv} ~/Unity/Hub/Editor/${version}/Editor/Unity "$@"
  '';

in {
  unity-editor = unity-editor;
  devShell = mkShell { packages = [ unity-editor ]; };
}
