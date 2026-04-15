import {
  type SamplerConfig,
  samplerPresetDefault,
  samplerPresetDry,
  samplerPresetGrammar,
  samplerPresetGreedy,
  samplerPresetJson,
  samplerPresetTemperature,
  samplerPresetTopK,
  samplerPresetTopP,
} from "../generated/ts/nobodywho";

/**
 * Static factory methods for common sampler configurations.
 *
 * @example
 * ```typescript
 * const sampler = SamplerPresets.temperature(0.7);
 * const chat = new Chat(model, undefined, 4096, undefined, undefined, sampler);
 * ```
 */
export class SamplerPresets {
  private constructor() {}

  /** Get the default sampler configuration. */
  static default(): SamplerConfig {
    return samplerPresetDefault();
  }

  /** Create a sampler with top-k filtering only. */
  static topK(topK: number): SamplerConfig {
    return samplerPresetTopK(topK);
  }

  /** Create a sampler with nucleus (top-p) sampling. */
  static topP(topP: number): SamplerConfig {
    return samplerPresetTopP(topP);
  }

  /** Create a greedy sampler (always picks most probable token). */
  static greedy(): SamplerConfig {
    return samplerPresetGreedy();
  }

  /** Create a sampler with temperature scaling. */
  static temperature(temperature: number): SamplerConfig {
    return samplerPresetTemperature(temperature);
  }

  /** Create a DRY sampler preset to reduce repetition. */
  static dry(): SamplerConfig {
    return samplerPresetDry();
  }

  /** Create a sampler configured for JSON output generation. */
  static json(): SamplerConfig {
    return samplerPresetJson();
  }

  /** Create a sampler with a custom grammar constraint. */
  static grammar(grammar: string): SamplerConfig {
    return samplerPresetGrammar(grammar);
  }
}
