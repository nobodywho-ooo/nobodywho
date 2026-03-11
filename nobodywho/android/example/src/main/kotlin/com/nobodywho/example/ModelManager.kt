package com.nobodywho.example

/**
 * Model paths for the example app.
 *
 * Push models to the device before running:
 *   adb push model.gguf     /data/local/tmp/model.gguf
 *   adb push embedder.gguf  /data/local/tmp/embedder.gguf
 *
 * Any llama.cpp-compatible GGUF model works. Tested with:
 *   Chat:     Qwen/Qwen3-0.6B-GGUF  (qwen3-0.6b-q4_k_m.gguf)
 *   Embedder: nomic-ai/nomic-embed-text-v1.5-GGUF  (nomic-embed-text-v1.5.Q4_K_M.gguf)
 */
object ModelManager {
    const val CHAT_MODEL_PATH    = "/data/local/tmp/model.gguf"
    const val EMBEDDER_MODEL_PATH = "/data/local/tmp/embedder.gguf"
}
