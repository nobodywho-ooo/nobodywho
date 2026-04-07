/**
 * NobodyWho — React Native bindings for local LLM inference.
 *
 * This is the public entry point. It re-exports the public API:
 * - Wrapper classes: Chat, Tool, TokenStream (wrapping internal Rust* types)
 * - Direct re-exports: Model, Encoder, CrossEncoder, SamplerBuilder, SamplerConfig, etc.
 * - Utilities: SamplerPresets, loadModel, cosineSimilarity
 *
 * This file is NOT generated — it is safe to edit.
 */

// Wrapper classes that hide the internal Rust* types.
export { Chat } from "./chat";
export { Tool } from "./tool";
export { TokenStream } from "./streaming";

// Re-export types that don't need wrapping.
export {
  Model,
  Encoder,
  CrossEncoder,
  SamplerBuilder,
  SamplerConfig,
  Role,
  Message,
  Message_Tags,
  PromptPart,
  PromptPart_Tags,
  cosineSimilarity,
  loadModel,
  uniffiInitAsync,
} from "./index";

export type {
  ModelInterface,
  EncoderInterface,
  CrossEncoderInterface,
  SamplerBuilderInterface,
  SamplerConfigInterface,
  Asset,
  ToolCall,
  ToolParameter,
} from "./index";

// Ergonomic wrapper additions.
export { SamplerPresets } from "./sampler_presets";
