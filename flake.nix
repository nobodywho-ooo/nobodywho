{
  description = "NobodyWho - a godot plugin for NPC dialogue with local LLMs";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs = { nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
    let
      pkgs = (import nixpkgs { system = system; });
      nobodywho = pkgs.callPackage ./nobodywho/godot {};
      godot-integration-test = pkgs.callPackage ./nobodywho/godot/integration-test { inherit nobodywho; };
      run-godot-integration-test = pkgs.runCommand "checkgame" { nativeBuildInputs = with pkgs; [ mesa.drivers ]; } ''
        cd ${godot-integration-test}
        export HOME=$TMPDIR
        ./game --headless
        touch $out
      '';
    in
  {
      packages.default = nobodywho;
      packages.godot-integration-test = godot-integration-test;
      checks.default = run-godot-integration-test;
      devShells.default = import ./nobodywho/shell.nix { inherit pkgs; };
  });
}
