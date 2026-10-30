/**
 * Instantiates the Emscripten module, and sets up its filesystem.
 *
 * Shared by the entry point (`index.wasm.ts`) and the `fetch()`-based downloader
 * (`download.ts`), which writes into the filesystem. Only used on WASM.
 */

import createEmscriptenModule from "./uniffi.js";
import type { EmscriptenFS, EmscriptenModule } from "./uniffi.js";

// Under Node.js, expose the host filesystem at the same absolute paths, so that
// e.g. `Model.load({ modelPath: "/path/to/model.gguf" })` works.
//
// We mount each top-level host directory with NODEFS, since the root itself
// can't be mounted over. (`-lnoderawfs.js` would do this, but doesn't work in
// browsers.) `/dev` and `/proc` are left as Emscripten's own.
function mountHostFilesystem(FS: EmscriptenFS, NODEFS: unknown) {
  const nodeProcess = (globalThis as any).process;
  const { readdirSync, realpathSync, statSync } = nodeProcess.getBuiltinModule("node:fs");
  for (const name of readdirSync("/")) {
    if (name === "dev" || name === "proc") continue;
    try {
      const path = "/" + name;
      if (!statSync(path).isDirectory()) continue;
      try {
        FS.mkdir(path);
      } catch {
        // Already exists.
      }
      // NODEFS `lstat`s the root, so it must not be a symlink (e.g. `/tmp` on macOS).
      FS.mount(NODEFS, { root: realpathSync(path) }, path);
    } catch {
      // Not accessible, skip it.
    }
  }
  FS.chdir(nodeProcess.cwd());
}

// In browsers, persist `/tmp` (where downloaded models are cached) in
// IndexedDB, so it survives page reloads.
function mountPersistentStorage({
  FS,
  IDBFS,
  addRunDependency,
  removeRunDependency,
}: EmscriptenModule) {
  // `autoPersist` writes changes back to IndexedDB as files are modified.
  FS.mount(IDBFS, { autoPersist: true }, "/tmp");

  // Load the persisted files before the module starts running.
  // If IndexedDB is unavailable (e.g. blocked by browser settings), carry on
  // with `/tmp` as plain in-memory storage.
  addRunDependency("idbfs");
  const loaded = (err?: unknown) => {
    if (err) console.error("failed to load persistent storage:", err);
    removeRunDependency("idbfs");
  };
  try {
    FS.syncfs(true, loaded);
  } catch (err) {
    loaded(err);
  }
}

// Instantiate the WASM module using Emscripten's logic, which also provides
// the JS side of the imports it needs (WebGPU, filesystem, etc.).
//
// We don't use the generated `uniffiInitAsync` from `@ubjs/wasm`, as that
// instantiates the module itself, and doesn't know about Emscripten's imports.
export default await createEmscriptenModule({
  preRun: [
    (module) => {
      if ((globalThis as any).process?.versions?.node) {
        mountHostFilesystem(module.FS, module.NODEFS);
      } else {
        mountPersistentStorage(module);
      }
    },
  ],
});
