{ fetchurl }:
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
  TEST_MULTIMODAL_MODEL = fetchurl {
    name = "gemma-4-E2B-it-Q4_K_M.gguf";
    url = "https://huggingface.co/NobodyWho/Google_Gemma4-E2B-GGUF/resolve/main/gemma-4-E2B-it-Q4_K_M.gguf";
    sha256 = "sha256-k3i8RxcQIp7xZXCbYuNL+2IjFCDdr21ynnJzBbW4Zy0=";
  };
  TEST_MMPROJ_MODEL = fetchurl {
    name = "mmproj-BF16.gguf";
    url = "https://huggingface.co/NobodyWho/Google_Gemma4-E2B-GGUF/resolve/main/mmproj-BF16.gguf";
    sha256 = "sha256-pALxD7V4C/kdA6EM2JBhE59SK+4uZ5sSkbv9zXHZVH0=";
  };
}
