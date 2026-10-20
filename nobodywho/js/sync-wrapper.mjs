#!/usr/bin/env node
// Copies the wrapper sources from `wasm/src/wrapper/` to `react-native/src/wrapper/`.
//
// These are shared between the bindings.
//
// The duplication is unfortunate, but I don't see a better solution:
// - Symlinked files resolve `import` relative to the source, not the symlink.
// - Putting the files in a separate package would complicate things.
// - Using `require` works worse with bundlers IIUC.
// - Doing a post-build copy or similar would still require the sources in the package.
// - Doing a pre-pack copy would make local development harder.
//
// Usage: `npm run sync-wrapper`

import { readdirSync, readFileSync, writeFileSync, mkdirSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const JS = dirname(fileURLToPath(import.meta.url));
const SRC = join(JS, "wasm", "src", "wrapper");
const DEST = join(JS, "react-native", "src", "wrapper");

const sources = readdirSync(SRC).filter((f) => f.endsWith(".ts")).sort();

rmSync(DEST, { recursive: true, force: true });
mkdirSync(DEST, { recursive: true });
for (const file of sources) {
  const header =
    `// Copied from nobodywho/js/wasm/src/wrapper/${file} by nobodywho/js/sync-wrapper.mjs.\n` +
    `// Do not edit: edit the original and run \`npm run sync-wrapper\` in nobodywho/js.\n\n`;
  writeFileSync(join(DEST, file), header + readFileSync(join(SRC, file), "utf8"));
}
console.log(`synced ${sources.length} files from wasm/src/wrapper/ to react-native/src/wrapper/`);
