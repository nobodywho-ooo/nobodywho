package com.nobodywho

import java.io.Closeable
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import uniffi.nobodywho.RustTokenStream as InternalRustTokenStream

/**
 * A stream of response tokens from the model.
 *
 * ```kotlin
 * chat.ask("Hello!").asFlow().collect { token -> print(token) }
 * // Or get the complete response:
 * val response = chat.ask("Hello!").completed()
 * ```
 */
class TokenStream internal constructor(
    internal val inner: InternalRustTokenStream
) : Closeable {
    suspend fun nextToken(): String? = inner.nextToken()
    suspend fun completed(): String = inner.completed()

    fun asFlow(): Flow<String> = flow {
        while (true) {
            currentCoroutineContext().ensureActive()
            val token = inner.nextToken() ?: break
            emit(token)
        }
        inner.completed()
    }

    /** Free the underlying Rust resources. */
    fun destroy() = inner.destroy()
    override fun close() { destroy() }
}
