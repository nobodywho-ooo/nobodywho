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
      nobodywho = pkgs.callPackage ./nobodywho {};
      integration-test = pkgs.callPackage ./integration-test { inherit nobodywho; };
      run-integration-test = pkgs.runCommand "checkgame" { nativeBuildInputs = with pkgs; [ mesa.drivers ]; } ''
        cd ${integration-test}
        export HOME=$TMPDIR
        ./game --headless
        touch $out
      '';
    in
  {
      packages.default = nobodywho;
      packages.integration-test = integration-test;
      checks.default = run-integration-test;
      devShells.default = import ./nobodywho/shell.nix { inherit pkgs; };
  });
}
