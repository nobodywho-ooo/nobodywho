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
export { downloadModel } from "./download";

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
