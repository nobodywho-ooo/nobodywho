import { RustTranslator } from "../generated/ts/nobodywho";
import { Model } from "./model";
import { TokenStream } from "./streaming";

/**
 * A stateless one-shot translator using a TranslateGemma-compatible model.
 *
 * Each `Translator` is bound to a single language pair (source → target).
 * Calls to `translate()` are stateless — context is reset before each translation.
 *
 * @example
 * ```typescript
 * const translator = await Translator.fromPath({
 *   modelPath: "translate-gemma.gguf",
 *   source: "en",
 *   target: "da",
 * });
 * for await (const token of translator.translate("Hello, how are you?")) {
 *   process.stdout.write(token);
 * }
 * ```
 */
export class Translator {
  /** @internal */
  private readonly _inner: RustTranslator;

  constructor(opts: {
    model: Model;
    source: string;
    target: string;
    contextSize?: number;
  }) {
    this._inner = new RustTranslator(
      opts.model._inner,
      opts.source,
      opts.target,
      opts.contextSize ?? undefined,
    );
  }

  /**
   * Create a translator directly from a model path.
   * Loads the model and creates the translator in one step.
   */
  static async fromPath(opts: {
    modelPath: string;
    source: string;
    target: string;
    useGpu?: boolean;
    contextSize?: number;
    onDownloadProgress?: (downloaded: number, total: number) => void;
  }): Promise<Translator> {
    const model = await Model.load({
      modelPath: opts.modelPath,
      useGpu: opts.useGpu,
      onDownloadProgress: opts.onDownloadProgress,
    });
    return new Translator({ model, ...opts });
  }

  /**
   * Translate the given text.
   *
   * Returns a {@link TokenStream} that yields translation tokens as they are generated.
   * The stream supports async iteration and `nextToken()` / `completed()` methods.
   */
  translate(text: string): TokenStream {
    return new TokenStream(this._inner.translate(text));
  }

  /**
   * Immediately free the underlying Rust resources.
   * After calling this, the Translator instance is no longer usable.
   */
  destroy(): void {
    this._inner.uniffiDestroy();
  }
}
