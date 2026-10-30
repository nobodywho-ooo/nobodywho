import { defineConfig } from "tsdown";

// Builds the WASM entry point (used by browsers and Node.js) into `dist/`.
//
// The React Native entry point is not built, Metro consumes `src/` directly.
export default defineConfig({
  entry: { index: "src/index.wasm.ts" },
  // Runs in both browsers and Node.js.
  platform: "neutral",
  target: "es2022",
  dts: true,
  sourcemap: true,
  // package.json points at `.mjs` / `.d.mts`.
  fixedExtension: true,
  deps: {
    // The Emscripten glue imports Node.js builtins when running in Node.js.
    neverBundle: [/^node:/],
  },
  checks: {
    // The Emscripten glue uses `eval` to implement `emscripten_run_script`.
    eval: false,
  },
  // The Emscripten glue (bundled) loads `uniffi.wasm` from beside itself.
  copy: ["src/uniffi.wasm"],
});
