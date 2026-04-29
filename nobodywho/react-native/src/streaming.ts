import type { RustTokenStreamInterface } from "../generated/ts/nobodywho";

/**
 * A stream of response tokens from the model.
 *
 * Wraps the internal RustTokenStream and provides an async iterable interface
 * so it can be used with `for await`.
 *
 * @example
 * ```typescript
 * const stream = chat.ask("Hello!");
 * for await (const token of stream) {
 *   process.stdout.write(token);
 * }
 * // Or get the complete response:
 * const stream = chat.ask("Hello!");
 * const fullResponse = await stream.completed();
 * ```
 */
export class TokenStream implements AsyncIterable<string> {
  /** @internal */
  readonly _inner: RustTokenStreamInterface;

  /** @internal */
  constructor(inner: RustTokenStreamInterface) {
    this._inner = inner;
  }

  /** Get the next token. Returns undefined when generation is complete. */
  async nextToken(): Promise<string | undefined> {
    return this._inner.nextToken();
  }

  /** Wait for the full response to complete and return it. */
  async completed(): Promise<string> {
    return this._inner.completed();
  }

  /** Async iterator support for `for await (const token of stream)`. */
  async *[Symbol.asyncIterator](): AsyncIterableIterator<string> {
    let token: string | undefined;
    while ((token = await this._inner.nextToken()) !== undefined) {
      yield token;
    }
  }
}
