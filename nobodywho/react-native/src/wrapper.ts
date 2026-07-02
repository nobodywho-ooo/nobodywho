/**
 * NobodyWho — React Native bindings for local LLM inference.
 *
 * This is the public entry point. It re-exports the public API:
 * - Wrapper classes: Model, Chat, Encoder, CrossEncoder, Tts, Tool, TokenStream, Prompt
 * - Direct re-exports: SamplerBuilder, SamplerConfig
 * - Types: Message, Asset, ToolCall
 * - Utilities: SamplerPresets, cosineSimilarity, downloadModel
 *
 * This file is NOT generated — it is safe to edit.
 */

// Native crate install + bindings initialize. These run as side effects of
// evaluating this module, which the React Native runtime is guaranteed to do
// before any FFI call because wrapper.ts is the package main.
//
// The install logic was previously reached only through lazy re-exports from
// ./index. In release mode Metro's inlineRequires defers lazy re-exports
// until first use, so consumers whose first interaction was Model.load /
// downloadModel / Chat never triggered ./index and FFI calls then threw
// "Cannot read property 'ubrn_…' of undefined". Inlining the install here
// removes the ./index indirection so this can't recur.
import installer from "./NativeNobodywho";
import nobodywhoDefault, {
  downloadModel as _downloadModel,
} from "../generated/ts/nobodywho";

installer.installRustCrate();
nobodywhoDefault.initialize();

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
export { STT } from "./stt";
export { Encoder } from "./encoder";
export { CrossEncoder } from "./cross_encoder";
export { Tts } from "./tts";
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
} from "../generated/ts/nobodywho";

export type {
  Asset,
  ToolCall,
  CachedModel,
  ChatStats,
} from "../generated/ts/nobodywho";
export type { TtsBackend, TtsDevice, TtsOptions } from "./tts";

// Ergonomic wrapper additions.
export { SamplerPresets } from "./sampler_presets";
