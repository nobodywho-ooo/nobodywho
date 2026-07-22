{ fetchurl, runCommand, fetchgit }:
{
  TEST_MODEL = fetchurl {
    name = "Qwen_Qwen3-0.6B-Q4_K_M.gguf";
    url = "https://huggingface.co/NobodyWho/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf";
    sha256 = "sha256-Fopf1gkSVBieJZR2TbHGbdaymE8AkfkqhvOIvd0dm7I=";
  };
  # Recurrent / hybrid-recurrent model used to verify the checkpoint restore
  # path in `test_checkpoint_restore_fires_on_recurrent_model`. The tiny
  # Qwen3.5 variant is small enough to keep in CI while still exhibiting the
  # Gated Delta Networks that break naive partial `clear_kv_cache_seq`.
  TEST_RECURRENT_MODEL = fetchurl {
    name = "Qwen_Qwen3.5-2B-Q4_K_M-vendor-sampling.gguf";
    url = "https://huggingface.co/NobodyWho/Qwen_Qwen3.5-2B-GGUF/resolve/main/Qwen_Qwen3.5-2B-Q4_K_M-vendor-sampling.gguf";
    sha256 = "sha256-3iqa3kOoguEsomShdal7XEHL1tyA09AyhB4OhKjGS5s=";
  };
  TEST_EMBEDDINGS_MODEL = fetchurl {
    name = "bge-small-en-v1.5-q8_0.gguf";
    url = "https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/main/bge-small-en-v1.5-q8_0.gguf";
    sha256 = "sha256-7Djo2hQllrqpExJK5QVQ3ihLaRa/WVd+8vDLlmDC9RQ=";
  };
  TEST_CROSSENCODER_MODEL = fetchurl {
    name = "bge-reranker-v2-m3-Q8_0.gguf";
    url = "https://huggingface.co/gpustack/bge-reranker-v2-m3-GGUF/resolve/main/bge-reranker-v2-m3-Q8_0.gguf";
    sha256 = "sha256-pDx8mxGkwVF+W/lRUZYOFiHRty96STNksB44bPGqodM=";
  };
  TEST_VISION_MODEL = fetchurl {
    name = "gemma-3-4b-it-Q4_K_M.gguf";
    url = "https://huggingface.co/unsloth/gemma-3-4b-it-GGUF/resolve/main/gemma-3-4b-it-Q4_K_M.gguf";
    sha256 = "sha256-BKQ6IujSAD3tpazCYvaOwQBfp2xzWpliqMdwQqdKfRk=";
  };
  TEST_MMPROJ_MODEL = fetchurl {
    name = "mmproj-gemma-3-f16.gguf";
    url = "https://huggingface.co/unsloth/gemma-3-4b-it-GGUF/resolve/main/mmproj-F16.gguf";
    sha256 = "sha256-cxGZ4BbsXyJ7gpP++DmJlHLg7kxRrfX55ctm9lWPoUI=";
  };

  # onnx-community/whisper-base — multi-file ONNX repo assembled into a local dir.
  # The STT tests request quantization="default" (fp32), so the local dir
  # must contain the unsuffixed ONNX files:
  # onnx/encoder_model.onnx, onnx/decoder_model_merged.onnx,
  # tokenizer.json, config.json, generation_config.json.
  TEST_WHISPER_MODEL = runCommand "whisper-base-model" { } ''
    mkdir -p $out/onnx
    cp ${fetchurl {
      name = "encoder_model.onnx";
      url = "https://huggingface.co/onnx-community/whisper-base/resolve/main/onnx/encoder_model.onnx";
      sha256 = "sha256-qfO3UoM7SeiA3ske5bbZNhEr58PqB8IhAkukk0OfRv4=";
    }} $out/onnx/encoder_model.onnx
    cp ${fetchurl {
      name = "decoder_model_merged.onnx";
      url = "https://huggingface.co/onnx-community/whisper-base/resolve/main/onnx/decoder_model_merged.onnx";
      sha256 = "sha256-UUkDdEuxtFgD7Fca+ZsxEQSRxvd7ChVIJYZplfsSS3M=";
    }} $out/onnx/decoder_model_merged.onnx
    cp ${fetchurl {
      name = "tokenizer.json";
      url = "https://huggingface.co/onnx-community/whisper-base/resolve/main/tokenizer.json";
      sha256 = "sha256-J/xHa/5/FymUgL4ic/wGCOTVqZq6KrXexTdLRILRpWY=";
    }} $out/tokenizer.json
    cp ${fetchurl {
      name = "config.json";
      url = "https://huggingface.co/onnx-community/whisper-base/resolve/main/config.json";
      sha256 = "sha256-9NBgj32RgWbafts+GI3l7xv+cNmALnhdJx/YgRHpz0s=";
    }} $out/config.json
    cp ${fetchurl {
      name = "generation_config.json";
      url = "https://huggingface.co/onnx-community/whisper-base/resolve/main/generation_config.json";
      sha256 = "sha256-YQcM+N4lseklbo4QLe1J2NJKg2ntNu+E/fIVSeaBJaA=";
    }} $out/generation_config.json
  '';

  # The whole whisper-base repo laid out as the HF download cache expects
  # (see `OnnxSource` in core/src/huggingface.rs): $out/onnx-community/whisper-base/
  # with a `.nobodywho-complete` marker so it resolves offline, no network.
  TEST_WHISPER_HF_CACHE = runCommand "whisper-base-hf-cache"
    {
      outputHashAlgo = "sha256";
      outputHashMode = "recursive";
      outputHash = "sha256-FfrrlOafv++1nqhhvXhfUi2jJ5jr6llC+9ALR9GWiIk=";
    }
    ''
      dir=$out/onnx-community/whisper-base
      mkdir -p "$out/onnx-community"
      cp -r ${fetchgit {
        name = "whisper-base";
        url = "https://huggingface.co/onnx-community/whisper-base";
        rev = "1846881b6b3a3024392c1eea3ad983695bc23925";
        fetchLFS = true;
        hash = "sha256-UDkez1Ix3HR7IE9g6WqG8Fuwd9w0VBP9/7yt6bbG7y4=";
      }} $dir
      chmod -R u+w $dir
      (cd $dir && find . -type f -printf '%P\n' | sort) > $dir/.nobodywho-complete
    '';
}
