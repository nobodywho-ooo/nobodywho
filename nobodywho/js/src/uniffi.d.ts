// Types for the Emscripten JS glue.
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
  locateFile?: (path: string, scriptDirectory: string) => string;
  [key: string]: unknown;
}

export default function createEmscriptenModule(
  moduleArg?: EmscriptenModuleArg,
): Promise<EmscriptenModule>;

export interface EmscriptenFS {
  lookupPath(path: string, opts?: {}): unknown;
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
