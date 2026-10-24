import { RustDecision } from "../generated/ts/nobodywho";
import { Model } from "./model";

/**
 * A question asked of a decision model. `choice`, `score`, and `noul` (boolean)
 * map directly to the Laya question types.
 */
export type DecisionQuestion =
  | {
      type: "choice";
      instructions: unknown;
      criteria: Record<string, unknown>;
    }
  | {
      type: "score";
      instructions: unknown;
      criteria: unknown[];
    }
  | {
      type: "noul";
      instructions: unknown;
      criteria?: Record<string, unknown>;
    };

export type DecisionRequest = {
  state: unknown;
  questions: Record<string, DecisionQuestion>;
};

export type DecisionAnswer =
  | {
      type: "choice";
      choice: string;
      probabilities: Record<string, number>;
      confidence: number;
    }
  | {
      type: "score";
      score: number;
      legend: Record<string, unknown>;
      probabilities: Record<string, number>;
      confidence: number;
    }
  | { type: "noul"; noul: number };

export type DecisionResponse = {
  answers: Record<string, DecisionAnswer>;
  usage: { input_tokens: number; output_tokens: number };
};

/**
 * Typed, non-generative decisions over text or JSON using a Laya GGUF model.
 * Questions run independently; every answer comes with probabilities instead
 * of generated text.
 *
 * @example
 * ```typescript
 * const decision = await Decision.fromPath({
 *   modelPath: "hf://ggml-org/Laya-GGUF/Laya-Q8_0.gguf",
 * });
 * const response = await decision.decide({
 *   state: "I was charged twice. Please refund the duplicate payment.",
 *   questions: {
 *     refund: { type: "noul", instructions: "Is a refund requested?" },
 *   },
 * });
 * console.log(response.answers.refund);
 * ```
 */
export class Decision {
  /** @internal */
  private readonly _inner: RustDecision;

  constructor(opts: { model: Model; contextSize?: number }) {
    this._inner = new RustDecision(opts.model._inner, opts.contextSize);
  }

  /**
   * Create a decision handle directly from a model path.
   * Loads the model and creates the handle in one step.
   */
  static async fromPath(opts: {
    modelPath: string;
    useGpu?: boolean;
    contextSize?: number;
    onDownloadProgress?: (downloaded: number, total: number) => void;
  }): Promise<Decision> {
    const model = await Model.load({
      modelPath: opts.modelPath,
      useGpu: opts.useGpu,
      onDownloadProgress: opts.onDownloadProgress,
    });
    return new Decision({ model, contextSize: opts.contextSize });
  }

  /** Run the request's questions over its state. */
  async decide(request: DecisionRequest): Promise<DecisionResponse> {
    const responseJson = await this._inner.decide(
      JSON.stringify(request.state),
      JSON.stringify(request.questions),
    );
    return JSON.parse(responseJson);
  }

  /**
   * Immediately free the underlying Rust resources.
   * After calling this, the Decision instance is no longer usable.
   */
  destroy(): void {
    this._inner.uniffiDestroy();
  }
}
