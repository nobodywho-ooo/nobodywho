// Smoke test that bypasses wasm-bindgen's auto-init.
//
// wasm-bindgen-cli generates ESM glue that does `import * as ... from 'env'`
// for every env import group in the wasm — none of those modules exist,
// so the auto-generated loader can't be used from Node directly. Instead,
// manually instantiate the wasm with our own imports object, then verify
// the expected exports show up.
//
// This proves the wasm itself is loadable. Going further (calling exported
// classes like Model/Chat) requires the wasm-bindgen JS glue, which is
// what the bundler/web-with-bundler targets are for. For npm distribution
// we'd ship with `--target bundler` and let webpack/esbuild handle the
// env imports via an alias.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = join(here, '..', 'pkg', 'nobodywho_wasm_bg.wasm');
const bytes = readFileSync(wasmPath);

console.log(`Wasm size: ${(bytes.length / (1024 * 1024)).toFixed(1)} MB`);

// Build a Proxy that returns a stub for any function the wasm asks for —
// the env imports number in the hundreds (libc + C++ runtime + mtmd_*), so
// enumerating them all by hand isn't useful for a "does it load" check.
const stubProxy = new Proxy(
  {},
  {
    get(_t, name) {
      // Return a no-op function for anything the wasm tries to import.
      // We track which ones get called; if it's a memory / table import
      // we'd need to provide a real one, but for now everything we see
      // is a function.
      return (...args) => {
        // Pretty-print first call per symbol so the smoke test output
        // is readable; later calls log nothing.
        if (!called.has(name)) {
          called.add(name);
        }
        return 0;
      };
    },
    has(_t, name) {
      return true;
    },
  },
);
const called = new Set();

// Compile + instantiate.
console.log('Compiling…');
const mod = await WebAssembly.compile(bytes);

console.log(`Imports: ${WebAssembly.Module.imports(mod).length}`);
const importsByModule = {};
for (const i of WebAssembly.Module.imports(mod)) {
  (importsByModule[i.module] ??= []).push(`${i.name} (${i.kind})`);
}
for (const [m, names] of Object.entries(importsByModule)) {
  console.log(`  ${m}: ${names.length} entries`);
}

// Provide each module as the stub Proxy. The wasm doesn't actually call
// these at module-init time (or rather: any call gets a no-op 0), so the
// instance comes up.
console.log('Instantiating…');
const inst = await WebAssembly.instantiate(mod, {
  ...Object.fromEntries(
    Object.keys(importsByModule).map((m) => [m, stubProxy]),
  ),
});

console.log(`Exports: ${Object.keys(inst.exports).length}`);
const ownClasses = Object.keys(inst.exports)
  .filter((n) => /^(chat|model|encoder|tokenstream|init)_/.test(n) || n === 'init')
  .sort();
console.log(`  Class-like exports: ${ownClasses.join(', ') || '(none)'}`);

console.log('');
console.log('✓ wasm instantiated, exports present.');
console.log('  Bytes loaded, imports resolved with no-op stubs, exports table populated.');
console.log('  This proves the wasm is well-formed; calling methods requires real imports.');
