# pyke's prebuilt ONNX Runtime: the same static library `ort`'s
# `download-binaries` feature links into release builds. The nix sandbox has no
# network, so we fetch it here and point ort-sys at it via ORT_LIB_PATH.
# When bumping `ort`, copy the URLs and hashes from ort-sys's
# build/download/dist.tsv, picking the feature set ort-sys would choose.
{
  lib,
  stdenvNoCC,
  fetchurl,
  xz,
}:

let
  version = "1.28.0";
  dists = {
    # `cuda` is enabled on x86_64 Linux (see core/Cargo.toml).
    x86_64-linux = {
      file = "x86_64-unknown-linux-gnu+cuda13,tensorrt,nvrtx";
      sha256 = "b89451babb9d4ec77c3f381e8ef92b640e125021b59f9cc7747e73c9fe8d549b";
    };
    aarch64-linux = {
      file = "aarch64-unknown-linux-gnu";
      sha256 = "06a050ab9137ccb32421d0cb49e9ccf72d9e18ab0aeb8f8d038d1b5cc844b35a";
    };
    aarch64-darwin = {
      file = "aarch64-apple-darwin+coreml";
      sha256 = "6934874e2e953576d9c1db47ff1af39c62c4f4220dbe6f988e131f72879674c7";
    };
  };
  system = stdenvNoCC.hostPlatform.system;
  dist = dists.${system} or (throw "no prebuilt ONNX Runtime for ${system}");
in
stdenvNoCC.mkDerivation {
  pname = "onnxruntime-pyke";
  inherit version;

  src = fetchurl {
    # Commas aren't allowed in store path names, so name the file ourselves.
    name = "onnxruntime-${version}-${system}.tar.lzma2";
    url = "https://cdn.pyke.io/0/pyke:ort-rs/ms@${version}/${dist.file}.tar.lzma2";
    inherit (dist) sha256;
  };

  nativeBuildInputs = [ xz ];
  dontUnpack = true;

  # A raw LZMA2 stream (no xz container), as ort-sys decodes it.
  installPhase = ''
    mkdir -p $out/lib
    xz --format=raw --lzma2=dict=64MiB -dc $src | tar -x -C $out/lib
  '';

  # The CUDA provider libraries link against CUDA, which isn't in the closure;
  # they are only dlopen'ed when CUDA is requested.
  dontFixup = true;

  meta.platforms = lib.attrNames dists;
}
