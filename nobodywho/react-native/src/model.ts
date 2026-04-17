import {
  loadModel,
  type ModelInterface,
} from "../generated/ts/nobodywho";

/**
 * A loaded LLM model that can be shared across Chat, Encoder, and CrossEncoder instances.
 *
 * @example
 * ```typescript
 * // Load from a local file
 * const model = await Model.load({ modelPath: "model.gguf" });
 * const chat = new Chat({ model, systemPrompt: "You are helpful." });
 *
 * // Download from HuggingFace (cached automatically)
 * const model = await Model.load({
 *   modelPath: "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf",
 * });
 *
 * // With vision support
 * const visionModel = await Model.load({
 *   modelPath: "model.gguf",
 *   projectionModelPath: "mmproj.gguf",
 * });
 * ```
 */
export class Model {
  /** @internal */
  readonly _inner: ModelInterface;

  /** @internal */
  private constructor(inner: ModelInterface) {
    this._inner = inner;
  }

  /**
   * Load a GGUF model from a local path or remote URL.
   *
   * @param opts.modelPath - Path to the GGUF model file, or a `hf://owner/repo/file.gguf` / `https://` URL to download and cache automatically
   * @param opts.useGpu - Use GPU acceleration if available (default: true)
   * @param opts.projectionModelPath - Path to a multimodal projector file for vision models
   */
  static async load(opts: {
    modelPath: string;
    useGpu?: boolean;
    projectionModelPath?: string;
  }): Promise<Model> {
    const inner = await loadModel(
      opts.modelPath,
      opts.useGpu ?? true,
      opts.projectionModelPath,
    );
    return new Model(inner);
  }

  /**
   * Immediately free the underlying Rust resources (model weights, GPU memory).
   * After calling this, the Model instance is no longer usable.
   */
  destroy(): void {
    (this._inner as any).uniffiDestroy();
  }
}
