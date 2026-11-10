/**
 * Instantiates the Emscripten module, and sets up its filesystem.
 */

import createEmscriptenModule from "./uniffi.js";
import type { EmscriptenModule } from "./uniffi.js";

// Mount a filesystem over each top-level directory, so that absolute paths
// (e.g. `Model.load({ modelPath: "/path/to/model.gguf" })`) work everywhere.
//
// The root itself can't be mounted over, hence each top-level directory.
// `/dev` and `/proc` are left as Emscripten's own.
//
// Under Node.js, the host's directories are mounted with NODEFS. In browsers,
// Emscripten's default directories (`/tmp` and `/home`) are persisted in
// IndexedDB with IDBFS, so that e.g. downloaded models survive page reloads.
function mountFilesystems({
  FS,
  NODEFS,
  IDBFS,
  addRunDependency,
  removeRunDependency,
}: EmscriptenModule) {
  const nodeProcess = (globalThis as any).process;
  const nodeFs = nodeProcess?.versions?.node
    ? nodeProcess.getBuiltinModule("node:fs")
    : null;

  const names: string[] = nodeFs ? nodeFs.readdirSync("/") : FS.readdir("/");
  for (const name of names) {
    if (name === "." || name === ".." || name === "dev" || name === "proc")
      continue;
    const path = "/" + name;
    try {
      if (nodeFs) {
        // Skips dangling symlinks (e.g. `/.VolumeIcon.icns` on macOS).
        const stat = nodeFs.statSync(path, { throwIfNoEntry: false });
        if (!stat?.isDirectory()) continue;
        FS.mkdirTree(path);
        // NODEFS `lstat`s the root, so it must not be a symlink (e.g. `/tmp` on macOS).
        FS.mount(NODEFS, { root: nodeFs.realpathSync(path) }, path);
      } else {
        if (!FS.isDir(FS.stat(path).mode)) continue;
        // `autoPersist` writes changes back to IndexedDB as files are modified.
        FS.mount(IDBFS, { autoPersist: true }, path);
      }
    } catch (err) {
      console.error(`failed to mount filesystem at ${path}:`, err);
    }
  }

  if (nodeFs) {
    FS.chdir(nodeProcess.cwd());
    return;
  } else {
    // Load the persisted files before the module starts running.
    // If IndexedDB is unavailable (e.g. blocked by browser settings), carry on
    // with plain in-memory storage.
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
}

// Instantiate the WASM module using Emscripten's logic, which also provides
// the JS side of the imports it needs (WebGPU, filesystem, etc.).
//
// We don't use the generated `uniffiInitAsync` from `@ubjs/wasm`, as that
// instantiates the module itself, and doesn't know about Emscripten's imports.
export default await createEmscriptenModule({
  preRun: [mountFilesystems],
});
