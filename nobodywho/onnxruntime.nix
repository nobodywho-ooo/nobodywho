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
  version = "1.31.0";
  dists = {
    # `cuda` is enabled on x86_64 Linux (see core/Cargo.toml).
    x86_64-linux = {
      file = "x86_64-unknown-linux-gnu+cuda13,tensorrt,nvrtx";
      sha256 = "535872087d5a5921aad090fdb46c65bb11c4c50b900dfcf6657ad3d5c465986f";
    };
    aarch64-linux = {
      file = "aarch64-unknown-linux-gnu";
      sha256 = "380c1b63eedf5671e7b39e4aa0f6c377ecc351c3a7b01ae708a707e73fb54709";
    };
    aarch64-darwin = {
      file = "aarch64-apple-darwin+coreml";
      sha256 = "4033efca0e6c1606bd2ec3870d4cb19d0dba0ad0160dbfb616e66cae969ed68c";
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
