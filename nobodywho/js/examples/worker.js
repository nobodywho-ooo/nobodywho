// Web Worker entry for browser-chat.html. The protocol dispatcher
// (load-model / create-chat / ask) used to live here as ~50 lines of JS;
// it now lives in Rust as `runInWorker` in js/src/lib.rs. Importing
// setup-browser.mjs runs the wasm bootstrap and, inside a Worker context,
// hands `self.onmessage` over to Rust.
import './setup-browser.mjs';
