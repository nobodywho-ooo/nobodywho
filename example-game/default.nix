{ nobodywho, stdenv, fetchurl, godot_4, godot_4-export-templates }:
let
  model = fetchurl {
    name = "gemma-2-2b-it-Q4_K_M.gguf";
    url = "https://huggingface.co/bartowski/gemma-2-2b-it-GGUF/resolve/main/gemma-2-2b-it-Q4_K_M.gguf";
    hash = "sha256-4K7oUGDxaPDy2Ec9fqQc4vMjDBvBN0hHUF6lmSiKd4c=";
  };
  embedding_model = fetchurl {
    name = "bge-small-en-v1.5-q8_0.gguf";
    url = "https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/main/bge-small-en-v1.5-q8_0.gguf";
    hash = "sha256-7Djo2hQllrqpExJK5QVQ3ihLaRa/WVd+8vDLlmDC9RQ=";
  };
in
stdenv.mkDerivation {
  name = "nobodywho example game";
  src = ./.;
  buildPhase = ''
    # setup stuff godot needs: export templates
    export HOME=$TMPDIR
    mkdir -p $HOME/.local/share/godot/export_templates
    ln -s ${godot_4-export-templates} $HOME/.local/share/godot/export_templates/4.3.stable

    # copy in gdextension stuff
    rm ./nobodywho.gdextension
    mkdir -p ./bin/
    cp ${nobodywho}/lib/libnobodywho.so ./bin/libnobodywho.so
    cat << EOF > bin/nobodywho.gdextension
    [configuration]
    entry_symbol = "gdext_rust_init"
    compatibility_minimum = 4.3
    reloadable = true

    [libraries]
    linux.debug.x86_64 =     "res://bin/libnobodywho.so"
    linux.release.x86_64 =   "res://bin/libnobodywho.so"
    EOF

    # build game
    mkdir -p $out
    ${godot_4}/bin/godot4 --verbose --headless --export-debug "Linux" $out/game
    ${godot_4}/bin/godot4 --verbose --headless --export-debug "Linux" $out/game

    cp ${model} $out/gemma-2-2b-it-Q4_K_M.gguf
    cp ${embedding_model} $out/bge-small-en-v1.5-q8_0.gguf

    # Patch binaries.
    patchelf --set-interpreter $(cat $NIX_CC/nix-support/dynamic-linker) $out/game
  '';
}
