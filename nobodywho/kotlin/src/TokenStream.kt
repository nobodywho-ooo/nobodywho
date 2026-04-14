package com.nobodywho

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import uniffi.nobodywho.RustTokenStream as InternalRustTokenStream

/**
 * A stream of response tokens from the model.
 *
 * Wraps the internal `RustTokenStream` and provides a Kotlin [Flow] interface.
 *
 * ```kotlin
 * val stream = chat.ask("Hello!")
 * stream.asFlow().collect { token ->
 *     print(token)
 * }
 * // Or get the complete response:
 * val fullResponse = chat.ask("Hello!").completed()
 * ```
 */
class TokenStream internal constructor(
    internal val inner: InternalRustTokenStream
) {
    /** Get the next token. Returns null when generation is complete. */
    suspend fun nextToken(): String? = inner.nextToken()

    /** Wait for the full response to complete and return it. */
    suspend fun completed(): String = inner.completed()

    /** Get a [Flow] of tokens for use with Kotlin coroutines. */
    fun asFlow(): Flow<String> = flow {
        while (true) {
            val token = inner.nextToken() ?: break
            emit(token)
        }
    }
}
