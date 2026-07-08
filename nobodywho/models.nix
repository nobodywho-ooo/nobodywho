{ fetchurl, runCommand }:
{
  TEST_MODEL = fetchurl {
    name = "Qwen_Qwen3-0.6B-Q4_K_M.gguf";
    url = "https://huggingface.co/NobodyWho/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf";
    sha256 = "sha256-Fopf1gkSVBieJZR2TbHGbdaymE8AkfkqhvOIvd0dm7I=";
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
  # The Rust backend expects (q4 is DEFAULT_QUANTIZATION in
  # nobodywho/core/src/stt/backends/whisper.rs — keep these in sync):
  # onnx/encoder_model_q4.onnx, onnx/decoder_model_merged_q4.onnx,
  # tokenizer.json, config.json, generation_config.json.
  TEST_WHISPER_MODEL = runCommand "whisper-base-model" { } ''
    mkdir -p $out/onnx
    cp ${fetchurl {
      name = "encoder_model_q4.onnx";
      url = "https://huggingface.co/onnx-community/whisper-base/resolve/main/onnx/encoder_model_q4.onnx";
      sha256 = "sha256-nU6cMehVnnHePg5J1guDCVFIRpnvU3R6SgYmG7vIy4g=";
    }} $out/onnx/encoder_model_q4.onnx
    cp ${fetchurl {
      name = "decoder_model_merged_q4.onnx";
      url = "https://huggingface.co/onnx-community/whisper-base/resolve/main/onnx/decoder_model_merged_q4.onnx";
      sha256 = "sha256-Cfg7cc7tyX2rHZC5FHFdxTKmRqFHq9oR2Dxkhnt8MZw=";
    }} $out/onnx/decoder_model_merged_q4.onnx
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
}
