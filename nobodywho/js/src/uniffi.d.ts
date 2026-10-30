// Types for the Emscripten JS glue in `./uniffi.js`, generated with
// `-sMODULARIZE=1 -sEXPORT_ES6=1` by `uniffi/build.rs`.
export interface EmscriptenModule {
  // Requested with `-sEXPORTED_RUNTIME_METHODS=wasmExports`.
  wasmExports: WebAssembly.Exports;
  // Filesystem types, for use with `FS.mount`.
  NODEFS: unknown;
  IDBFS: unknown;
  addRunDependency(id: string): void;
  removeRunDependency(id: string): void;
}

export interface EmscriptenModuleArg {
  preRun?: Array<(module: EmscriptenModule) => void>;
  [key: string]: unknown;
}

export default function createEmscriptenModule(
  moduleArg?: EmscriptenModuleArg,
): Promise<EmscriptenModule>;
