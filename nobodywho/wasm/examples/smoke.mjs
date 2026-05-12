// Smoke test for the wasm-bindgen --target web output, run under Node.
//
// Node 18+ has `fetch` and `import.meta.url`, so the web-target ESM module
// works almost as-is. The only wrinkle is the 23 unresolved env imports
// (mtmd_*, _Unwind_*, dlclose) that the wasm has — wasm-bindgen's default
// init() doesn't take an imports override, so we instantiate the wasm
// ourselves with the env stubs and then call __wbg_set_wasm to wire it up.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const pkgDir = join(here, '..', 'pkg');

const stubs = {};
const stub = (name) => (...args) => {
  throw new Error(`unresolved env.${name} called (args: ${args.join(',')})`);
};
for (const fn of [
  'mtmd_default_marker', 'mtmd_bitmap_free', 'mtmd_bitmap_get_data',
  'mtmd_bitmap_get_n_bytes', 'mtmd_bitmap_is_audio', 'mtmd_bitmap_set_id',
  'mtmd_helper_bitmap_init_from_file', 'mtmd_helper_eval_chunks',
  'mtmd_helper_get_n_tokens', 'mtmd_input_chunk_free',
  'mtmd_input_chunk_get_id', 'mtmd_input_chunk_get_n_tokens',
  'mtmd_input_chunk_get_type', 'mtmd_input_chunks_free',
  'mtmd_input_chunks_get', 'mtmd_input_chunks_size',
  'mtmd_input_chunks_init', 'mtmd_free', 'mtmd_tokenize',
  '_Unwind_RaiseException', '_Unwind_DeleteException', '_Unwind_CallPersonality',
  'dlclose',
]) {
  stubs[fn] = stub(fn);
}

console.log('Smoke test: nobodywho-wasm');
console.log('  Loading wasm-bindgen glue + wasm bytes…');

// Polyfill: the web-target glue uses fetch(url) for the wasm file. Replace
// with fs.readFileSync via a synchronous wrapper.
const realFetch = globalThis.fetch;
globalThis.fetch = async (urlOrReq) => {
  const url = typeof urlOrReq === 'string' ? urlOrReq : urlOrReq.url;
  if (url.startsWith('file://') && url.endsWith('.wasm')) {
    const path = fileURLToPath(url);
    const bytes = readFileSync(path);
    return new Response(bytes, { headers: { 'content-type': 'application/wasm' } });
  }
  return realFetch ? realFetch(urlOrReq) : Promise.reject(new Error('no fetch'));
};

const mod = await import(join(pkgDir, 'nobodywho_wasm.js'));

// wasm-bindgen 0.2's default export accepts a `module_or_path` arg plus an
// optional `imports` callback (see source comments in nobodywho_wasm.js).
// The callback receives the default imports object and returns the final one.
console.log('  Instantiating wasm with env stubs…');
await mod.default({
  imports: (defaultImports) => ({
    ...defaultImports,
    env: { ...(defaultImports.env || {}), ...stubs },
  }),
});

console.log('  ✓ Wasm loaded');

console.log('  Calling init() (panic hook + tracing)…');
mod.init();
console.log('  ✓ init ok');

console.log('  Checking exposed classes…');
const have = ['Model', 'Chat', 'TokenStream', 'Encoder'].filter((n) => n in mod);
console.log(`  ✓ found: ${have.join(', ')}`);

console.log('');
console.log('Smoke test passed. Wasm initializes; classes exposed.');
console.log('');
console.log('Next: load a GGUF via Model.loadBytes() and call chat.ask().');
