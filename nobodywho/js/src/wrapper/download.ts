import {
    downloadModel as _downloadModel,
    parseModelPath,
    GgufSource_Tags,
} from "../generated";

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
    const headersMap = opts.headers
        ? new Map(Object.entries(opts.headers))
        : undefined;
    const cb = opts.onDownloadProgress;
    const progressCallback = cb
        ? { onDownloadProgress: (d: bigint, t: bigint) => cb(Number(d), Number(t)) }
        : undefined;
    const source = parseModelPath(opts.modelPath);
    if (source.tag === GgufSource_Tags.Local) {
        return source.inner[0];
    }
    return await _downloadModel(source.inner[0], headersMap, progressCallback);
}
