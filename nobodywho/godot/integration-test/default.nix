{ nobodywho-godot, stdenv, fetchurl, godot_4, godot_4-export-templates-bin, fontconfig }:
let
  model = fetchurl {
    name = "Qwen_Qwen3-0.6B-Q4_0.gguf";
    url = "https://huggingface.co/bartowski/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_0.gguf";
    hash = "sha256-S3jY48YZds67eO9a/+GdDsp1sbR+xm9hOloyRUhHWNU=";
  };

  embedding_model = fetchurl {
    name = "bge-small-en-v1.5-q8_0.gguf";
    url = "https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/main/bge-small-en-v1.5-q8_0.gguf";
    hash = "sha256-7Djo2hQllrqpExJK5QVQ3ihLaRa/WVd+8vDLlmDC9RQ=";
  };

  reranker_model = fetchurl {
    name = "bge-reranker-v2-m3-Q8_0.gguf";
    url = "https://huggingface.co/gpustack/bge-reranker-v2-m3-GGUF/resolve/main/bge-reranker-v2-m3-Q8_0.gguf";
    hash = "sha256-0OdXMicV+utzNaRezjkufM67NOMudXQ4bqdbniOCAKo=";
  };
in
stdenv.mkDerivation {
  name = "nobodywho example game";
  src = ./.;
  buildInputs = [ fontconfig ];
  buildPhase = ''
    # setup stuff godot needs: export templates
    export HOME=$TMPDIR
    mkdir -p $HOME/.local/share/godot/export_templates
    ln -s ${godot_4-export-templates-bin}/share/godot/export_templates/4.4.1.stable $HOME/.local/share/godot/export_templates/4.4.1.stable

    # copy in gdextension stuff
    rm ./nobodywho.gdextension
    mkdir -p ./bin/
    cp ${nobodywho-godot}/lib/libnobodywho_godot.so ./bin/libnobodywho_godot.so
    cat << EOF > bin/nobodywho.gdextension
    [configuration]
    entry_symbol = "gdext_rust_init"
    compatibility_minimum = 4.3
    reloadable = true

    [libraries]
    linux.debug.x86_64 =     "res://bin/libnobodywho_godot.so"
    linux.release.x86_64 =   "res://bin/libnobodywho_godot.so"
    EOF

    # build game
    mkdir -p $out
    ${godot_4}/bin/godot4 --verbose --headless --import
    ${godot_4}/bin/godot4 --verbose --headless --export-debug "Linux" $out/game

    cp ${model} $out/Qwen_Qwen3-0.6B-Q4_0.gguf
    cp ${embedding_model} $out/bge-small-en-v1.5-q8_0.gguf
    cp ${reranker_model} $out/bge-reranker-v2-m3-Q8_0.gguf

    # Patch binaries.
    patchelf --set-interpreter $(cat $NIX_CC/nix-support/dynamic-linker) $out/game
  '';
}
