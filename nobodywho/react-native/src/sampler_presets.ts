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
    return samplerPresetDefault() as SamplerConfig;
  }

  /** Create a sampler with top-k filtering only. */
  static topK(topK: number): SamplerConfig {
    return samplerPresetTopK(topK) as SamplerConfig;
  }

  /** Create a sampler with nucleus (top-p) sampling. */
  static topP(topP: number): SamplerConfig {
    return samplerPresetTopP(topP) as SamplerConfig;
  }

  /** Create a greedy sampler (always picks most probable token). */
  static greedy(): SamplerConfig {
    return samplerPresetGreedy() as SamplerConfig;
  }

  /** Create a sampler with temperature scaling. */
  static temperature(temperature: number): SamplerConfig {
    return samplerPresetTemperature(temperature) as SamplerConfig;
  }

  /** Create a DRY sampler preset to reduce repetition. */
  static dry(): SamplerConfig {
    return samplerPresetDry() as SamplerConfig;
  }

  /** Create a sampler configured for JSON output generation. */
  static json(): SamplerConfig {
    return samplerPresetJson() as SamplerConfig;
  }

  /** Create a sampler with a custom grammar constraint. */
  static grammar(grammar: string): SamplerConfig {
    return samplerPresetGrammar(grammar) as SamplerConfig;
  }
}
