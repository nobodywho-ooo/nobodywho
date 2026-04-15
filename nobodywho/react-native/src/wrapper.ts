/**
 * NobodyWho — React Native bindings for local LLM inference.
 *
 * This is the public entry point. It re-exports the public API:
 * - Wrapper classes: Chat, Tool, TokenStream (wrapping internal Rust* types)
 * - Wrapper classes: Encoder, CrossEncoder (wrapping internal types)
 * - Direct re-exports: Model, SamplerBuilder, SamplerConfig, etc.
 * - Utilities: SamplerPresets, loadModel, cosineSimilarity
 *
 * This file is NOT generated — it is safe to edit.
 */

// Wrapper classes that hide the internal Rust* types.
export { Chat } from "./chat";
export { Encoder } from "./encoder";
export { CrossEncoder } from "./cross_encoder";
export { Prompt } from "./prompt";
export { Tool } from "./tool";
export { TokenStream } from "./streaming";

// Re-export types that don't need wrapping.
export {
  Model,
  SamplerBuilder,
  SamplerConfig,
  Role,
  Message,
  Message_Tags,
  cosineSimilarity,
  loadModel,
} from "./index";

export type {
  ModelInterface,
  SamplerBuilderInterface,
  SamplerConfigInterface,
  Asset,
  ToolCall,
  ToolParameter,
} from "./index";

// Ergonomic wrapper additions.
export { SamplerPresets } from "./sampler_presets";
