{
  description = "NobodyWho - a godot plugin for NPC dialogue with local LLMs";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs = { nixpkgs, flake-utils, crane, ... }:
    flake-utils.lib.eachDefaultSystem (system:
    let
      pkgs = (import nixpkgs { system = system; });
      craneLib = crane.mkLib pkgs;
      nobodywho = pkgs.callPackage ./nobodywho { inherit craneLib; };
      integration-test = pkgs.callPackage ./example-game { inherit nobodywho; };
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
