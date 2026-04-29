"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.streamTokens = streamTokens;
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
async function* streamTokens(stream) {
    let token;
    while ((token = await stream.nextToken()) !== null) {
        yield token;
    }
}
