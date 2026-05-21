{
  "name": "@nobodywho/js",
  "version": "0.0.0-PLACEHOLDER",
  "description": "Run local LLMs in the browser via llama.cpp compiled to WebAssembly.",
  "license": "MIT",
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
  "types": "./nobodywho_js.d.ts",
  "type": "module",
  "files": [
    "nobodywho_js.js",
    "nobodywho_js.d.ts",
    "nobodywho_js_bg.js",
    "nobodywho_js_bg.wasm",
    "nobodywho_js_bg.wasm.d.ts",
    "README.md"
  ],
  "sideEffects": [
    "./nobodywho_js.js",
    "./snippets/*"
  ],
  "engines": {
    "node": ">=20"
  }
}
