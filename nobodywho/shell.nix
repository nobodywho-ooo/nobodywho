{ pkgs ? import <nixpkgs> {}, ... }: 

let 
  nobodywho = pkgs.callPackage ./default.nix {};
  unity-editor = nobodywho.unity-editor;

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

    # these are the dependencies required by llama.cpp to build for vulkan
    # (these packages were discovered by looking at the nix source code in ggerganov/llama.cpp)
    pkgs.vulkan-headers
    pkgs.vulkan-loader
    pkgs.shaderc
  ];
  shellHook = ''
    ulimit -n 2048
  '';
}
