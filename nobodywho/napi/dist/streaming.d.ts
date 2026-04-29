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
export declare function streamTokens(stream: TokenStream): AsyncIterableIterator<string>;
