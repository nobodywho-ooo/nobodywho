{
  "name": "@nobodywho/wasm",
  "version": "0.1.0",
  "description": "Run local LLMs in the browser via llama.cpp compiled to WebAssembly.",
  "license": "MIT",
  "repository": {
    "type": "git",
    "url": "https://github.com/nobodywho-ooo/nobodywho",
    "directory": "nobodywho/wasm"
  },
  "homepage": "https://nobodywho.ooo",
  "keywords": [
    "llm",
    "llama",
    "llama.cpp",
    "wasm",
    "webassembly",
    "local",
    "inference",
    "gguf"
  ],
  "main": "./nobodywho_wasm.js",
  "types": "./nobodywho_wasm.d.ts",
  "type": "module",
  "files": [
    "nobodywho_wasm.js",
    "nobodywho_wasm.d.ts",
    "nobodywho_wasm_bg.js",
    "nobodywho_wasm_bg.wasm",
    "nobodywho_wasm_bg.wasm.d.ts",
    "README.md"
  ],
  "sideEffects": [
    "./nobodywho_wasm.js",
    "./snippets/*"
  ],
  "peerDependencies": {
    "@bjorn3/browser_wasi_shim": "^0.4.0"
  },
  "engines": {
    "node": ">=18"
  }
}
