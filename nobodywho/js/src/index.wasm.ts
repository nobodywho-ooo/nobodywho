/**
 * NobodyWho — WebAssembly entry point, used in browsers and (for now) Node.js.
 *
 * Bundled by tsdown into `dist/`, together with the Emscripten JS glue in
 * `./uniffi.js` (see `uniffi/build.rs`). The glue locates `uniffi.wasm`
 * relative to itself with `new URL("uniffi.wasm", import.meta.url)`, so the
 * `.wasm` file is copied into `dist/` beside the bundle.
 *
 * This file is NOT generated — it is safe to edit.
 */

import emscripten from "./emscripten";
import { UniffiNativeModule } from "@ubjs/wasm/core";
import { PLAYER_DEFINITIONS, setNativeModule } from "./generated/wasm/nobodywho-ffi.js";
import nobodywho from "./generated/wasm/nobodywho.js";

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
