/**
 * NobodyWho — WebAssembly entry point, used in browsers and (for now) Node.js.
 *
 * Bundled by tsdown into `dist/`, together with the Emscripten JS glue in
 * `build/uniffi.js` (see `uniffi/build.rs`). The glue locates `uniffi.wasm`
 * relative to itself with `new URL("uniffi.wasm", import.meta.url)`, so the
 * `.wasm` file is copied into `dist/` beside the bundle.
 *
 * This file is NOT generated — it is safe to edit.
 */

import createEmscriptenModule from "../build/uniffi.js";
import { UniffiNativeModule } from "@ubjs/wasm/core";
import {
  PLAYER_DEFINITIONS,
  setNativeModule,
} from "../generated/wasm/nobodywho-ffi";
import nobodywho from "../generated/wasm/nobodywho";

// Instantiate the WASM module using Emscripten's logic, which also provides
// the JS side of the imports it needs (WebGPU, filesystem, etc.).
//
// We don't use the generated `uniffiInitAsync` from `@ubjs/wasm`, as that
// instantiates the module itself, and doesn't know about Emscripten's imports.
const emscripten = await createEmscriptenModule();

// Tie that together with `@ubjs/wasm`'s player.
//
// TODO: Upstream a public `UniffiNativeModule.fromInstance()`, we hackily
// work around the constructor being private here. It only reads `exports`.
const nativeModule: UniffiNativeModule = new (UniffiNativeModule as any)({
  exports: emscripten.wasmExports,
});
setNativeModule(nativeModule.registerSync(PLAYER_DEFINITIONS));
nobodywho.initialize();

export * from "./wrapper";
