/**
 * Downloads models with `fetch()` into Emscripten's virtual filesystem.
 *
 * Replaces the Rust `downloadModel` on WASM (see `download.native.ts` for the other platforms), since Rust has no usable
 * HTTP client there. Path parsing and cache layout come from core, through `parseModelPath`.
 */

import { parseModelPath, ModelLocation } from "./generated/wasm/nobodywho";
import emscripten from "./emscripten";
import type { RustDownloadProgressCallback } from "./generated/wasm/nobodywho";

/** Same signature as the generated (Rust) `downloadModel`, but uses `fetch()`. */
export async function downloadModel(
  modelPath: string,
  headers: Map<string, string> | undefined,
  onDownloadProgress: RustDownloadProgressCallback | undefined,
  asyncOpts_?: { signal: AbortSignal },
): Promise<string> {
  const { FS } = emscripten;

  const location = parseModelPath(modelPath);
  if (ModelLocation.Local.instanceOf(location)) return location.inner.path;
  if (!ModelLocation.Remote.instanceOf(location)) throw new Error("unreachable");
  const { url, localPath: target } = location.inner;
  if (target.split(/[\\/]/).includes("..")) {
    throw new Error(`Invalid model path: ${modelPath}`);
  }

  if (FS.analyzePath(target).exists) return target;

  const response = await fetch(url, {
    headers: headers ? Object.fromEntries(headers) : undefined,
    signal: asyncOpts_?.signal,
  });
  if (!response.ok || !response.body) {
    throw new Error(
      `HTTP request failed (${response.status} ${response.statusText}): ${url}`,
    );
  }
  const total = BigInt(response.headers.get("content-length") ?? 0);

  FS.mkdirTree(target.slice(0, Math.max(target.lastIndexOf("/"), target.lastIndexOf("\\"))));
  const tmp = `${target}.part`;
  const stream = FS.open(tmp, "w");
  let done = false;
  try {
    const reader = response.body.getReader();
    let downloaded = 0n;
    let lastEmit = 0;
    for (; ;) {
      const { value, done: finished } = await reader.read();
      if (finished) break;
      FS.write(stream, value, 0, value.length);
      downloaded += BigInt(value.length);
      const now = Date.now();
      if (onDownloadProgress && now - lastEmit >= 100) {
        lastEmit = now;
        onDownloadProgress.onDownloadProgress(downloaded, total);
      }
    }
    onDownloadProgress?.onDownloadProgress(downloaded, total || downloaded);
    done = true;
  } finally {
    FS.close(stream);
    if (!done) FS.unlink(tmp);
  }
  FS.rename(tmp, target);
  return target;
}
