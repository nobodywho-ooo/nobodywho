/**
 * Downloads models with `fetch()` into Emscripten's virtual filesystem.
 *
 * Replaces the Rust `downloadModel` on WASM (see `download.native.ts` for the other platforms), since Rust has no usable
 * HTTP client there. Path parsing and cache layout come from core, through `parseModelPath`.
 */

import emscripten from "../emscripten";
import { GgufSource_Tags, parseModelPath } from "../generated";
import type { GgufSource, ModelPaths, ModelSources, RemoteFile, RustDownloadProgressCallback } from "../generated";

// Must match core's ModelCache root.
const CACHE_ROOT = "/tmp/nobodywho/models/";

/** Fetches `remote` into the virtual filesystem cache, returning the cached path. */
async function downloadFile(
    remote: RemoteFile,
    headers?: Record<string, string>,
    onProgress?: (downloaded: bigint, total: bigint) => void,
    signal?: AbortSignal,
): Promise<string> {
    const { FS } = emscripten;
    const target = CACHE_ROOT + remote.cachePath;

    try {
        FS.lookupPath(target);
        // If the file exists, it's already in the cache, so no need to download it again.
        return target;
    } catch {
        // Ignore the error.
    }

    const response = await fetch(remote.url, { headers, signal });
    if (!response.ok || !response.body) {
        throw new Error(`HTTP request failed (${response.status} ${response.statusText}): ${remote.url}`);
    }
    const total = BigInt(response.headers.get("content-length") ?? 0);

    // Stream the response to the IDBFS "disk".
    //
    // TODO: Optimize this using a custom filesystem (or maybe WasmFS?) to avoid the frequent copying here?
    FS.mkdirTree(target.slice(0, target.lastIndexOf("/")));
    const tmp = `${target}.part`;
    const stream = FS.open(tmp, "w");
    let finished = false;
    try {
        const reader = response.body.getReader();
        let downloaded = 0n;
        let lastEmit = 0;
        for (; ;) {
            const { value, done } = await reader.read();
            if (done) break;
            FS.write(stream, value, 0, value.length);
            downloaded += BigInt(value.length);
            const now = Date.now();
            if (onProgress && (now - lastEmit) >= 100) {
                lastEmit = now;
                onProgress(downloaded, total);
            }
        }
        onProgress?.(downloaded, total || downloaded);
        finished = true;
    } finally {
        FS.close(stream);
        if (!finished) FS.unlink(tmp);
    }
    FS.rename(tmp, target);
    return target;
}

/** Resolves a source to a local path, downloading it if remote. */
async function resolveSource(
    source: GgufSource,
    headers?: Record<string, string>,
    onProgress?: (downloaded: bigint, total: bigint) => void,
    signal?: AbortSignal,
): Promise<string> {
    if (source.tag === GgufSource_Tags.Local) {
        return source.inner[0];
    }
    return await downloadFile(source.inner[0], headers, onProgress, signal);
}

/**
 * Download a GGUF model from a remote URL or HuggingFace path and return the local file path.
 *
 * Use this when you need custom HTTP headers, e.g. for gated models that require
 * authentication. For unauthenticated downloads, pass the URL directly to `Model.load`.
 *
 * @param opts.modelPath - Path or URL to a GGUF model file (`hf://owner/repo/file.gguf`, `https://`, or a local path)
 * @param opts.headers - Optional HTTP headers (e.g. `{ Authorization: "Bearer hf_..." }`)
 * @param opts.onDownloadProgress - Optional callback receiving `(downloadedBytes, totalBytes)` during download
 * @returns The local filesystem path where the model was cached
 */
export async function downloadModel(opts: {
    modelPath: string;
    headers?: Record<string, string>;
    onDownloadProgress?: (downloaded: number, total: number) => void;
}): Promise<string> {
    const cb = opts.onDownloadProgress;
    const onProgress = cb ? (d: bigint, t: bigint) => cb(Number(d), Number(t)) : undefined;
    return await resolveSource(parseModelPath(opts.modelPath), opts.headers, onProgress, undefined);
}

/** Same signature as the generated (Rust) `downloadModels`, but uses `fetch()`. */
export async function downloadModels(
    sources: ModelSources,
    onDownloadProgress: RustDownloadProgressCallback | undefined,
    asyncOpts_?: { signal: AbortSignal },
): Promise<ModelPaths> {
    const signal = asyncOpts_?.signal;
    const onProgress = onDownloadProgress ? (d: bigint, t: bigint) => onDownloadProgress.onDownloadProgress(d, t) : undefined;
    const languageModel = await resolveSource(sources.languageModel, undefined, onProgress, signal);
    const projectionModel = sources.projectionModel ? await resolveSource(sources.projectionModel, undefined, onProgress, signal) : undefined;
    const draftModel = sources.draftModel ? await resolveSource(sources.draftModel, undefined, onProgress, signal) : undefined;
    return { languageModel, projectionModel, draftModel };
}
