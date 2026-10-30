/**
 * NobodyWho — JavaScript bindings for local LLM inference.
 *
 * This is the platform-independent public API, re-exported by the entry
 * points `index.react-native.ts` and `index.wasm.ts` once they've loaded
 * the native library. It re-exports:
 * - Wrapper classes: Model, Chat, Encoder, CrossEncoder, TextToSpeech, Tool, TokenStream, Prompt
 * - Direct re-exports: SamplerBuilder, SamplerConfig
 * - Types: Message, Content, ContentPart, ToolCall
 * - Utilities: SamplerPresets, cosineSimilarity, downloadModel
 */

import { downloadModel as _downloadModel } from "../generated";

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
export type { Options } from "./chat";
export { SpeechToText } from "./speech_to_text";
export { VoiceActivityDetection } from "./vad";
export { Encoder } from "./encoder";
export { Decision } from "./decision";
export type {
  DecisionAnswer,
  DecisionQuestion,
  DecisionRequest,
  DecisionResponse,
} from "./decision";
export { CrossEncoder } from "./cross_encoder";
export { TextToSpeech } from "./text_to_speech";
export { Prompt } from "./prompt";
export { Tool } from "./tool";
export { TokenStream } from "./streaming";

// Message types
export type { Message, Content, ContentPart } from "./message";

// Re-export types that don't need wrapping.
//
// Note: `getCachedModels` returns `size` as `bigint` (Rust `u64`). Wrap with
// `Number(m.size)` at the call site if a JS number is needed.
export {
  SamplerBuilder,
  SamplerConfig,
  ShiftTarget,
  cosineSimilarity,
  getCachedModels,
} from "../generated";

export type {
  ToolCall,
  CachedModel,
  ChatStats,
  ContextShiftOptions,
} from "../generated";
export type { TextToSpeechArchitecture, TextToSpeechDevice, TextToSpeechOptions } from "./text_to_speech";
export type { SpeechToTextOptions } from "./speech_to_text";
export { VoiceActivityDetectionEvent } from "./vad";
export type { VoiceActivityDetectionOptions } from "./vad";

// Ergonomic wrapper additions.
export { SamplerPresets } from "./sampler_presets";
