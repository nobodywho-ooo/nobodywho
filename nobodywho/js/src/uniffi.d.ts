// Types for the Emscripten JS glue in `./uniffi.js`, generated with
// `-sMODULARIZE=1 -sEXPORT_ES6=1` by `uniffi/build.rs`.
export interface EmscriptenModule {
  // Requested with `-sEXPORTED_RUNTIME_METHODS=wasmExports`.
  wasmExports: WebAssembly.Exports;
  // Requested with `-sEXPORTED_RUNTIME_METHODS=FS`.
  FS: EmscriptenFS;
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

export interface EmscriptenFS {
  analyzePath(path: string): { exists: boolean };
  mkdir(path: string): void;
  mkdirTree(path: string): void;
  chdir(path: string): void;
  open(path: string, flags: string): unknown;
  write(
    stream: unknown,
    buffer: Uint8Array,
    offset: number,
    length: number,
    position?: number,
  ): number;
  close(stream: unknown): void;
  rename(from: string, to: string): void;
  unlink(path: string): void;
  mount(type: unknown, opts: Record<string, unknown>, mountpoint: string): void;
  filesystems: Record<string, unknown>;
  syncfs(populate: boolean, callback: (err: unknown) => void): void;
}
