import { RustCrossEncoder } from "../generated/ts/nobodywho";
import { Model } from "./model";

/**
 * A cross-encoder for ranking documents by relevance to a query.
 *
 * Wraps the internal CrossEncoder with an ergonomic API including
 * a `fromPath` factory for one-step model loading.
 *
 * @example
 * ```typescript
 * const crossEncoder = await CrossEncoder.fromPath({
 *   modelPath: "crossencoder.gguf",
 * });
 * const scores = await crossEncoder.rank("query", ["doc1", "doc2"]);
 * const sorted = await crossEncoder.rankAndSort("query", ["doc1", "doc2"]);
 * ```
 */
export class CrossEncoder {
  /** @internal */
  private readonly _inner: RustCrossEncoder;

  constructor(opts: { model: Model; contextSize?: number }) {
    this._inner = new RustCrossEncoder(
      opts.model._inner,
      opts.contextSize ?? undefined,
    );
  }

  /**
   * Create a cross-encoder directly from a model path.
   * Loads the model and creates the cross-encoder in one step.
   */
  static async fromPath(opts: {
    modelPath: string;
    useGpu?: boolean;
    contextSize?: number;
    onDownloadProgress?: (downloaded: number, total: number) => void;
  }): Promise<CrossEncoder> {
    const model = await Model.load({
      modelPath: opts.modelPath,
      useGpu: opts.useGpu,
      onDownloadProgress: opts.onDownloadProgress,
    });
    return new CrossEncoder({ model, ...opts });
  }

  /** Rank documents by relevance to a query. Returns similarity scores. */
  async rank(query: string, documents: string[]): Promise<number[]> {
    return this._inner.rank(query, documents);
  }

  /**
   * Rank documents and return them sorted by descending relevance.
   * Returns an array of [document, score] pairs.
   */
  async rankAndSort(
    query: string,
    documents: string[],
  ): Promise<[string, number][]> {
    const json = await this._inner.rankAndSortJson(query, documents);
    return JSON.parse(json);
  }

  /**
   * Immediately free the underlying Rust resources.
   * After calling this, the CrossEncoder instance is no longer usable.
   */
  destroy(): void {
    this._inner.uniffiDestroy();
  }
}
