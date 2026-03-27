/**
 * NobodyWho — React Native bindings for local LLM inference.
 *
 * This is the public entry point. It re-exports everything from the
 * generated index.tsx (which handles native module initialization)
 * and adds ergonomic wrapper utilities.
 *
 * This file is NOT generated — it is safe to edit.
 */

// Re-export everything from the generated entry point.
// This triggers native module installation and binding initialization.
export {
  Model,
  Chat,
  TokenStream,
  Encoder,
  CrossEncoder,
  SamplerBuilder,
  SamplerConfig,
  Tool,
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
  ChatInterface,
  TokenStreamInterface,
  EncoderInterface,
  CrossEncoderInterface,
  SamplerBuilderInterface,
  SamplerConfigInterface,
  ToolInterface,
  Asset,
  ToolCall,
} from "./index";

// Ergonomic wrapper additions.
export { SamplerPresets } from "./sampler_presets";
export { streamTokens } from "./streaming";
export { createTool } from "./tool";
