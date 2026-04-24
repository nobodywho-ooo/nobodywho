{

  nobodywho-godot,
  stdenv,
  callPackage,
  godot_4,
  godot_4-export-templates-bin,
  fontconfig,
}:
let
  models = callPackage ../../models.nix { };
in
stdenv.mkDerivation {
  name = "nobodywho example game";
  src = ./.;
  buildInputs = [ fontconfig ];
  buildPhase = ''
    # setup stuff godot needs: export templates
    echo "Extracting export templates..."
    export HOME=$TMPDIR
    mkdir -p $HOME/.local/share
    ln -s ${godot_4-export-templates-bin}/share/godot $HOME/.local/share/godot
    echo "Finished extracting export templates."

    # copy in gdextension stuff
    echo "Setting up gdextension files..."
    rm ./nobodywho.gdextension
    mkdir -p ./bin/
    cp ${nobodywho-godot.lib}/lib/libnobodywho_godot.so ./bin/libnobodywho_godot.so
    cat << EOF > bin/nobodywho.gdextension
    [configuration]
    entry_symbol = "gdext_rust_init"
    compatibility_minimum = 4.3
    reloadable = true

    [libraries]
    linux.debug.x86_64 =     "res://bin/libnobodywho_godot.so"
    linux.release.x86_64 =   "res://bin/libnobodywho_godot.so"
    EOF
    echo "Finished setting up gdextension files"

    # build game
    mkdir -p $out
    echo "Running godot export..."

    # yes, this `|| true` is verifably insane
    # it's there because this export segfaults, but only on the github actions runner
    # (yes, the *exact same* flake works just fine on my local machine)
    # this kind internet stranger tells me that the export actually works fine, even though it segfaults
    # https://github.com/godotengine/godot/issues/112955#issuecomment-3554723333
    ${godot_4}/bin/godot4 --verbose --headless --export-debug "Linux" $out/game || true

    echo "Finished exporting godot game"

    cp ${models.TEST_MODEL} $out/Qwen_Qwen3-0.6B-Q4_0.gguf
    cp ${models.TEST_EMBEDDINGS_MODEL} $out/bge-small-en-v1.5-q8_0.gguf
    cp ${models.TEST_CROSSENCODER_MODEL} $out/bge-reranker-v2-m3-Q8_0.gguf

    # Pre-populate the HuggingFace download cache so the hf_path_test can resolve
    # `huggingface:NobodyWho/Qwen_Qwen3-0.6B-GGUF/...` without network access. Core's
    # download_file() checks this exact layout before hitting the network (see
    # `core/src/llm.rs::download_model_from_hf`). XDG_CACHE_HOME is pointed at
    # `$out/hf-cache` by the `run-godot-integration-test` derivation.
    mkdir -p $out/hf-cache/nobodywho/models/NobodyWho/Qwen_Qwen3-0.6B-GGUF
    ln -s ${models.TEST_MODEL} \
      $out/hf-cache/nobodywho/models/NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf

    # Patch binaries.
    patchelf --set-interpreter $(cat $NIX_CC/nix-support/dynamic-linker) $out/game
  '';
}
