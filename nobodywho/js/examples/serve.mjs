#!/usr/bin/env node
// Minimal static server for the browser examples. Cross-origin isolation —
// COOP + a COEP header — is what unlocks SharedArrayBuffer, and therefore
// Emscripten pthreads. That pair is the whole requirement.
//
// We use `credentialless` as the COEP value because it's the most forgiving
// minimum: cross-origin resources (e.g. the HuggingFace model fetch) load
// without having to send their own CORP headers. `require-corp` also works
// here. CORP and Cache-Control are intentionally omitted — the served
// HTML/JS/.wasm are same-origin, so neither affects isolation.
//
//   node examples/serve.mjs            # → http://localhost:8000/  (browser-chat.html)
//   PORT=3000 node examples/serve.mjs
import { createServer } from 'node:http';
import { createReadStream, statSync } from 'node:fs';
import { join, extname, normalize, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..'); // the js/ dir
const PORT = Number(process.env.PORT) || 8000;
const DEFAULT = '/examples/browser-chat.html';
const MIME = {
  '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript',
  '.wasm': 'application/wasm', '.json': 'application/json', '.css': 'text/css', '.map': 'application/json',
};

createServer((req, res) => {
  const headers = {
    'Cross-Origin-Opener-Policy': 'same-origin',
    'Cross-Origin-Embedder-Policy': 'credentialless',
  };
  let urlPath = decodeURIComponent(new URL(req.url, 'http://localhost').pathname);
  if (urlPath === '/') urlPath = DEFAULT;
  const filePath = join(ROOT, normalize(urlPath).replace(/^(\.\.[/\\])+/, ''));
  try {
    const st = statSync(filePath);
    if (st.isDirectory()) throw new Error('is a directory');
    res.writeHead(200, { ...headers, 'Content-Type': MIME[extname(filePath)] || 'application/octet-stream', 'Content-Length': st.size });
    createReadStream(filePath).pipe(res);
  } catch {
    res.writeHead(404, headers);
    res.end('not found');
  }
}).listen(PORT, () => {
  console.log(`serving ${ROOT}`);
  console.log(`→ open http://localhost:${PORT}/  (browser-chat.html, cross-origin isolated)`);
});
