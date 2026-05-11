import {
  type SamplerConfig,
  samplerPresetConstrainWithGrammar,
  samplerPresetConstrainWithJsonSchema,
  samplerPresetConstrainWithRegex,
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

  /** Create a sampler configured for JSON output generation (any valid JSON, GBNF-based). */
  static json(): SamplerConfig {
    return samplerPresetJson() as SamplerConfig;
  }

  /** Constrain output to a JSON schema via llguidance. */
  static constrainWithJsonSchema(schema: string): SamplerConfig {
    return samplerPresetConstrainWithJsonSchema(schema) as SamplerConfig;
  }

  /** Constrain output to a regular expression via llguidance. */
  static constrainWithRegex(pattern: string): SamplerConfig {
    return samplerPresetConstrainWithRegex(pattern) as SamplerConfig;
  }

  /**
   * Constrain output using a grammar via llguidance.
   * Accepts both Lark syntax (`start: "yes" | "no"`) and GBNF (`root ::= "yes" | "no"`).
   * GBNF is automatically converted to Lark before use.
   */
  static constrainWithGrammar(grammar: string): SamplerConfig {
    return samplerPresetConstrainWithGrammar(grammar) as SamplerConfig;
  }

  /**
   * @deprecated Use {@link constrainWithGrammar} instead. It accepts both Lark and GBNF strings.
   */
  static grammar(grammar: string): SamplerConfig {
    return samplerPresetGrammar(grammar) as SamplerConfig;
  }
}
