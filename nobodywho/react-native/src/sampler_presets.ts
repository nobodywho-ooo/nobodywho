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

  /** Constrain output to a JSON schema via llguidance. Accepts a schema object or a JSON string. */
  static constrainWithJsonSchema(schema: object | string): SamplerConfig {
    const schemaStr = typeof schema === "string" ? schema : JSON.stringify(schema);
    return samplerPresetConstrainWithJsonSchema(schemaStr) as SamplerConfig;
  }

  /** Constrain output to a regular expression via llguidance. Accepts a RegExp or a pattern string. */
  static constrainWithRegex(pattern: RegExp | string): SamplerConfig {
    const patternStr = pattern instanceof RegExp ? pattern.source : pattern;
    return samplerPresetConstrainWithRegex(patternStr) as SamplerConfig;
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
