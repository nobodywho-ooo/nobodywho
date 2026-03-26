import type { TokenStream } from "../index.js";

/**
 * Wrap a TokenStream as an AsyncIterableIterator, yielding each token as it
 * is generated. The iterator completes when the model finishes generating.
 *
 * @example
 * ```typescript
 * const stream = chat.ask("Hello!");
 * for await (const token of streamTokens(stream)) {
 *   process.stdout.write(token);
 * }
 * ```
 */
export async function* streamTokens(
  stream: TokenStream
): AsyncIterableIterator<string> {
  let token: string | null;
  while ((token = await stream.nextToken()) !== null) {
    yield token;
  }
}
