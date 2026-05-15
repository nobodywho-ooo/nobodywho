// CI smoke test. Importing setup.mjs loads the wasm, runs WASI + libc++
// static ctors, and registers the panic hook + tracing subscriber. If the
// import resolves without throwing, the wasm is wired up correctly.
//
// For real inference see chat_demo.mjs / encoder_demo.mjs in this directory.

import './setup.mjs';
console.log('✓ wasm loaded and initialized');
