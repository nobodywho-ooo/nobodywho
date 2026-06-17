/**
 * NobodyWho — React Native bindings for local LLM inference.
 *
 * This is the public entry point. It re-exports the public API:
 * - Wrapper classes: Model, Chat, Encoder, CrossEncoder, Tool, TokenStream, Prompt
 * - Direct re-exports: SamplerBuilder, SamplerConfig
 * - Types: Message, Asset, ToolCall
 * - Utilities: SamplerPresets, cosineSimilarity, downloadModel
 *
 * This file is NOT generated — it is safe to edit.
 */

// Side-effect import: forces ./index to evaluate at package load so
// installRustCrate() and the generated bindings' initialize() run before any
// FFI call. Without this, Metro's inlineRequires (release-only) defers ./index
// until a lazy re-export is touched, and FFI calls throw
// "Cannot read property 'ubrn_…' of undefined".
import "./index";

import { downloadModel as _downloadModel } from "../generated/ts/nobodywho";

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
  return _downloadModel(opts.modelPath, headersMap, progressCallback);
}

// Wrapper classes
export { Model } from "./model";
export { Chat } from "./chat";
export { Encoder } from "./encoder";
export { CrossEncoder } from "./cross_encoder";
export { Prompt } from "./prompt";
export { Tool } from "./tool";
export { TokenStream } from "./streaming";

// Message types
export type { Message } from "./message";

// Re-export types that don't need wrapping.
//
// Note: `getCachedModels` returns `size` as `bigint` (Rust `u64`). Wrap with
// `Number(m.size)` at the call site if a JS number is needed.
export {
  SamplerBuilder,
  SamplerConfig,
  cosineSimilarity,
  getCachedModels,
} from "./index";

export type {
  Asset,
  ToolCall,
  CachedModel,
  ChatStats,
} from "./index";

// Ergonomic wrapper additions.
export { SamplerPresets } from "./sampler_presets";
