{ fetchurl }:
{
  TEST_MODEL = fetchurl {
    name = "Qwen_Qwen3-0.6B-Q4_K_M.gguf";
    url = "https://huggingface.co/bartowski/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf";
    sha256 = "sha256-ms/B4AExHzS0JSABtiby5GbVkqQgZfZlcb/zeQ1OGxQ=";
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
}
