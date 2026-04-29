import { RustEncoder } from "../generated/ts/nobodywho";
import { Model } from "./model";

/**
 * An embedding encoder for generating text embeddings.
 *
 * Wraps the internal Encoder with an ergonomic API including
 * a `fromPath` factory for one-step model loading.
 *
 * @example
 * ```typescript
 * const encoder = await Encoder.fromPath({
 *   modelPath: "embeddings.gguf",
 * });
 * const embedding = await encoder.encode("Hello world");
 * ```
 */
export class Encoder {
  /** @internal */
  private readonly _inner: RustEncoder;

  constructor(opts: { model: Model; contextSize?: number }) {
    this._inner = new RustEncoder(opts.model._inner, opts.contextSize ?? undefined);
  }

  /**
   * Create an encoder directly from a model path.
   * Loads the model and creates the encoder in one step.
   */
  static async fromPath(opts: {
    modelPath: string;
    useGpu?: boolean;
    contextSize?: number;
    onDownloadProgress?: (downloaded: number, total: number) => void;
  }): Promise<Encoder> {
    const model = await Model.load({
      modelPath: opts.modelPath,
      useGpu: opts.useGpu,
      onDownloadProgress: opts.onDownloadProgress,
    });
    return new Encoder({ model, ...opts });
  }

  /** Encode text into an embedding vector. */
  async encode(text: string): Promise<number[]> {
    return this._inner.encode(text);
  }

  /**
   * Immediately free the underlying Rust resources.
   * After calling this, the Encoder instance is no longer usable.
   */
  destroy(): void {
    this._inner.uniffiDestroy();
  }
}
