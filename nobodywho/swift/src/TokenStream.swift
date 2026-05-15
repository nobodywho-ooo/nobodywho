import Foundation
import NobodyWhoGenerated

/// A stream of response tokens from the model.
///
/// Wraps the internal `RustTokenStream` and provides an `AsyncSequence` interface
/// so it can be used with `for try await`.
///
/// ```swift
/// let stream = chat.ask("Hello!")
/// for try await token in stream {
///     print(token, terminator: "")
/// }
/// // Or get the complete response:
/// let fullResponse = try await chat.ask("Hello!").completed()
/// ```
public struct TokenStream: AsyncSequence {
    public typealias Element = String

    let inner: RustTokenStream

    init(_ inner: RustTokenStream) {
        self.inner = inner
    }

    /// Get the next token. Returns nil when generation is complete, or throws if generation failed.
    public func nextToken() async throws -> String? {
        return try await inner.nextToken()
    }

    /// Wait for the full response to complete and return it.
    public func completed() async throws -> String {
        return try await inner.completed()
    }

    public func makeAsyncIterator() -> AsyncIterator {
        return AsyncIterator(inner: inner)
    }

    public struct AsyncIterator: AsyncIteratorProtocol {
        let inner: RustTokenStream

        public mutating func next() async throws -> String? {
            return try await inner.nextToken()
        }
    }
}
