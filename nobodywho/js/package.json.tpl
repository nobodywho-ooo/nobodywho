{
  "name": "@nobodywho/js",
  "version": "0.0.0-PLACEHOLDER",
  "description": "Run local LLMs in the browser via llama.cpp compiled to WebAssembly.",
  "license": "EUPL-1.2",
  "repository": {
    "type": "git",
    "url": "https://github.com/nobodywho-ooo/nobodywho",
    "directory": "nobodywho/js"
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
  "main": "./nobodywho_js.js",
  "type": "module",
  "files": [
    "nobodywho_js.js",
    "nobodywho_js.wasm",
    "README.md"
  ],
  "sideEffects": [
    "./nobodywho_js.js"
  ],
  "engines": {
    "node": ">=20"
  }
}
