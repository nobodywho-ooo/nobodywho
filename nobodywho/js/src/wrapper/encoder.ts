import { RustEncoder, type RustEncoderLike } from "../generated";
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
  private readonly _inner: RustEncoderLike;

  private constructor(inner: RustEncoderLike) {
    this._inner = inner;
  }

  /** Create from a loaded model asynchronously. */
  static async create(opts: { model: Model; contextSize?: number }): Promise<Encoder> {
    const inner = await RustEncoder.create(opts.model._inner, opts.contextSize ?? undefined);
    return new Encoder(inner);
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
    return Encoder.create({ model, contextSize: opts.contextSize });
  }

  /** Encode text into an embedding vector. */
  async encode(text: string): Promise<number[]> {
    return this._inner.encode(text);
  }

  /** Encode multiple texts, preserving input order. */
  async encodeBatch(texts: string[]): Promise<number[][]> {
    return this._inner.encodeBatch(texts);
  }

  /**
   * Immediately free the underlying Rust resources.
   * After calling this, the Encoder instance is no longer usable.
   */
  destroy(): void {
    (this._inner as any).uniffiDestroy();
  }
}
