/**
 * NobodyWho — wasm bindings for local LLM inference.
 */

// See also `uniffi/build.rs`, that contains a bunch of the

import createEmscriptenModule from "./uniffi.js";
import { UniffiNativeModule } from "@ubjs/wasm";
import { PLAYER_DEFINITIONS, setNativeModule } from "./generated/nobodywho-ffi";
import nobodywho from "./generated/nobodywho";

// Initialize and load the WASM blob using Emscripten's logic.
const emscripten = await createEmscriptenModule();

// Tie that together with @ubjs/wasm's logic.
//
// TODO: Upstream a public `UniffiNativeModule.fromInstance()`, we hackily
// work around it being private here.
const nativeModule: UniffiNativeModule = new (UniffiNativeModule as any)({
  exports: emscripten.wasmExports,
});
setNativeModule(nativeModule.registerSync(PLAYER_DEFINITIONS));
nobodywho.initialize();

export * from "./wrapper";
