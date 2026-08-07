{
  nobodywho-godot,
  stdenv,
  callPackage,
  godot_4,
  fontconfig,
  mesa,
}:
let
  models = callPackage ../../models.nix { };
in
stdenv.mkDerivation {
  name = "nobodywho-godot-tests";
  src = ./.;

  buildInputs = [
    fontconfig
  ];

  # The pre-built GDExtension .so from the flake's nobodywho-godot package.
  # We rewrite the .gdextension to point at it (the repo-relative path won't
  # exist inside the nix store derivation).
  preConfigure = ''
    # Drop the repo-relative .gdextension and write one that points at the
    # nix store's built library.
    rm nobodywho.gdextension
    cat << EOF > nobodywho.gdextension
    [configuration]
    entry_symbol = "gdext_rust_init"
    compatibility_minimum = 4.6
    reloadable = true

    [libraries]
    linux.debug.x86_64 =     "res://bin/libnobodywho_godot.so"
    linux.release.x86_64 =   "res://bin/libnobodywho_godot.so"
    EOF
  '';

  buildPhase = ''
    export HOME=$TMPDIR

    # Copy in the built GDExtension library.
    mkdir -p bin
    cp ${nobodywho-godot.lib}/lib/libnobodywho_godot.so bin/libnobodywho_godot.so

    # Pre-populate the HuggingFace download cache so TTS/STT tests that use
    # hf:// sources resolve offline. XDG_CACHE_HOME is set at run time.
    mkdir -p $out/hf-cache/nobodywho/models/NobodyWho
    ln -s ${models.TEST_MODEL} \
      $out/hf-cache/nobodywho/models/NobodyWho/Qwen_Qwen3-0.6B-Q4_K_M.gguf

    # Kokoro TTS — fetch from HF cache layout.
    # (If a TEST_TTS_SOURCE is not in the cache, the test self-skips, so this
    # is optional. We include the chat model + cross-encoder + encoder which
    # are the primary model-backed tests.)

    # Import the project (generates .godot/ cache so extension classes resolve).
    ${godot_4}/bin/godot --headless --import --path . || true

    # Run the test suite headless. The env vars point at the nix-fetched
    # models so the model-backed tests (chat, tools, encoder, crossencoder)
    # find their models without network access. TTS/STT self-skip if their
    # sources aren't set.
    TEST_MODEL=${models.TEST_MODEL} \
    TEST_ENCODER_MODEL=${models.TEST_EMBEDDINGS_MODEL} \
    TEST_CROSSENCODER_MODEL=${models.TEST_CROSSENCODER_MODEL} \
    XDG_CACHE_HOME=$out/hf-cache \
    ${godot_4}/bin/godot --headless --path .

    # Check the exit code — Godot exits 0 on all-pass, 1 on any failure.
    # The buildPhase fails if the command returns non-zero.
    touch $out
  '';

  # The "check" is the buildPhase itself — if the tests fail, the derivation
  # fails. No install step needed beyond $out existing.
}
