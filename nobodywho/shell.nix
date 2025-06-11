{ pkgs ? import <nixpkgs> {}, ... }: 

let 
  version = "6000.0.47f1";
  unity-editor = pkgs.writeShellScriptBin "unity-editor" ''
    ${pkgs.lib.getExe pkgs.unityhub.fhsEnv} ~/Unity/Hub/Editor/${version}/Editor/Unity "$@"
  '';

  mdbook-langtabs = pkgs.rustPlatform.buildRustPackage {
    name = "mdbook-langtabs";
    src = pkgs.fetchFromGitHub {
      owner = "nx10";
      repo = "mdbook-langtabs";
      rev = "v0.1.1";
      sha256 = "sha256-3Xr4np0OKq1l3oBnK1ChOWPMEUl+qtFVYx7niZ8PntE=";
    };
    cargoHash = "sha256-i8OsFuH6PDnifQXfHPX8qlpPPHi7ULPVkhDxfSQ6sZo=";
    # we do not want to test this.
    doCheck = false;
  };

in pkgs.mkShell {
  env.LIBCLANG_PATH = "${pkgs.libclang.lib}/lib/libclang.so";
  packages = [
    pkgs.cmake
    pkgs.clang
    pkgs.rustup
    
    # Unity
    unity-editor
    pkgs.unityhub
    pkgs.libxml2
    pkgs.just
    # lsp dependencies
    pkgs.dotnet-sdk_9
    pkgs.csharpier
    # used to make good stack traces
    pkgs.gdb
    pkgs.lldb

    # these are the dependencies required by llama.cpp to build for vulkan
    # (these packages were discovered by looking at the nix source code in ggerganov/llama.cpp)
    pkgs.vulkan-headers
    pkgs.vulkan-loader
    pkgs.shaderc

    pkgs.mdbook
    mdbook-langtabs
  ];
  shellHook = ''
    ulimit -n 2048
  '';
}
