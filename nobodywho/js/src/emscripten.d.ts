// Types for the Emscripten JS glue in `build/uniffi.js`, generated with
// `-sMODULARIZE=1 -sEXPORT_ES6=1` by `uniffi/build.rs`.
declare module "*/build/uniffi.js" {
  interface EmscriptenModule {
    // Requested with `-sEXPORTED_RUNTIME_METHODS=wasmExports`.
    wasmExports: WebAssembly.Exports;
  }

  export default function createEmscriptenModule(
    moduleArg?: Record<string, unknown>,
  ): Promise<EmscriptenModule>;
}
