// Run nobodywho-wasm under Node with a real WASI shim.
//
// Uses the `--target bundler` wasm-bindgen output but bypasses the missing
// bundler: we manually instantiate the .wasm, then call __wbg_set_wasm on
// the bg.js module so the exported classes (Chat, Model, ...) wire up.
//
// Usage:
//   node wasm/examples/run.mjs                            # smoke test (no model)
//   node wasm/examples/run.mjs path/to/model.gguf "your prompt"   # chat
//   node wasm/examples/run.mjs --encode embedding.gguf "your text" # embedding

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { WASI } from 'node:wasi';

const here = dirname(fileURLToPath(import.meta.url));
const pkgDir = join(here, '..', 'pkg-bundler');

const argv = process.argv.slice(2);
const encodeMode = argv[0] === '--encode';
const [modelPath, prompt] = encodeMode ? argv.slice(1) : argv;
const log = (...a) => console.log(...a);

// ---------- Imports object ----------
//
// Three groups (from wasm-tools dump): ./nobodywho_wasm_bg.js (the
// wasm-bindgen glue we'll fill from bg.js), env (unresolved C symbols —
// mtmd_*, _Unwind_*, dlclose), wasi_snapshot_preview1 (real WASI).

const bg = await import(join(pkgDir, 'nobodywho_wasm_bg.js'));

// node:wasi provides a fresh `WASI` instance whose getImportObject()
// returns the wasi_snapshot_preview1 implementation. Empty preopens =
// the wasm has no filesystem visibility, which matches the browser case.
const wasi = new WASI({ version: 'preview1', args: [], env: {} });

// Env stubs: the wasm imports mtmd_* (we don't call them, the binding
// doesn't expose multimodal), _Unwind_* (legacy exceptions; should not
// fire in normal operation), and dlclose. Throwing makes accidental
// calls visible during dev; switch to silent no-ops for prod.
const envStubs = new Proxy(
  {},
  {
    get(_t, name) {
      return (...args) => {
        throw new Error(`unresolved env.${String(name)}(${args.join(', ')})`);
      };
    },
  },
);

const wasmBytes = readFileSync(join(pkgDir, 'nobodywho_wasm_bg.wasm'));

log(`Wasm size: ${(wasmBytes.length / (1024 * 1024)).toFixed(1)} MB`);
log('Compiling…');
const mod = await WebAssembly.compile(wasmBytes);

log('Instantiating with WASI + bg.js glue + env stubs…');
const inst = await WebAssembly.instantiate(mod, {
  './nobodywho_wasm_bg.js': bg,
  env: envStubs,
  ...wasi.getImportObject(),
});

// node:wasi expects to be initialized against a reactor (no _start, has
// _initialize) or command (has _start). Our wasm exports `_initialize`
// when wasi-libc is linked, so wasi.initialize is the right call.
// IMPORTANT: only one of {wasi.initialize, manual __wasm_call_ctors,
// __wbindgen_start} should run — they all run C++ static constructors
// and register atexit handlers. Running twice produces signature-
// mismatched calls in __funcs_on_exit later.
//
// `wasi.initialize` runs `_initialize` which itself calls __wasm_call_ctors,
// so that covers libc + libc++ static init. We then run __wbindgen_start
// once to do wasm-bindgen's own startup (externref table allocation etc.).
try {
  wasi.initialize(inst);
  log('  ✓ wasi.initialize ran _initialize');
} catch (e) {
  log(`  ! wasi.initialize skipped (${e.message})`);
  // Fall back to running ctors manually.
  if (inst.exports.__wasm_call_ctors) inst.exports.__wasm_call_ctors();
}

// Wire wasm-bindgen's bg.js to the instantiated wasm.
bg.__wbg_set_wasm(inst.exports);

// wasm-bindgen's own startup — must run after __wasm_call_ctors.
if (inst.exports.__wbindgen_start) inst.exports.__wbindgen_start();

log('  ✓ wasm wired up');

// Try the simplest export.
log('Calling init() (panic hook + tracing)…');
try {
  bg.init();
  log('  ✓ init() ok');
} catch (e) {
  log(`  ✗ init() threw: ${e}`);
  console.error(e);
  process.exit(1);
}

if (!modelPath) {
  log('');
  log('Smoke test complete — wasm initializes under real WASI.');
  log('Pass a model path to load a model and run inference:');
  log('  node wasm/examples/run.mjs ./model.gguf "Hello, "');
  log('  node wasm/examples/run.mjs --encode ./embedding.gguf "some text"');
  process.exit(0);
}

// ---------- Real inference / embedding ----------

log(`Loading model from ${modelPath}…`);
const modelBytes = readFileSync(modelPath);
log(`  model size: ${(modelBytes.length / (1024 * 1024)).toFixed(1)} MB`);

const model = await bg.Model.loadBytes(new Uint8Array(modelBytes));
log('  ✓ model loaded');

if (encodeMode) {
  log('Creating Encoder…');
  const encoder = new bg.Encoder(model, 512);
  log('  ✓ encoder created');

  const text = prompt ?? 'The quick brown fox jumps over the lazy dog.';
  log(`Encoding: ${JSON.stringify(text)}`);
  const vec = await encoder.encode(text);
  log(`  ✓ embedding generated: ${vec.length} dimensions`);
  log(`  first 8: [${Array.from(vec.slice(0, 8)).map((v) => v.toFixed(4)).join(', ')}]`);
  log('Done.');
} else {
  log('Creating Chat…');
  const chat = new bg.Chat(model, { contextSize: 1024 });
  log('  ✓ chat created');

  const promptText = prompt ?? 'Hello, ';
  log(`Asking: ${JSON.stringify(promptText)}`);
  const stream = await chat.ask(promptText);

  process.stdout.write('Response: ');
  let tok;
  let count = 0;
  while ((tok = await stream.nextToken()) !== undefined && count < 64) {
    process.stdout.write(tok);
    count++;
  }
  process.stdout.write('\n');
  log(`  ✓ produced ${count} tokens`);
  log('Done.');
}
