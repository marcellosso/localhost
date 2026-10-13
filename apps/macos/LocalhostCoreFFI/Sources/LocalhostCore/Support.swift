import Foundation

// The thin Swift layer over the generated bindings (Generated/, written by
// scripts/build-core.sh). The generated types are the model; this layer adds
// what Swift clients expect of them: typed errors, `AsyncStream`s, Foundation
// types, and the methods the Swift core used to have.

// MARK: Typed errors

/// Runs a generated call, which throws untyped, rethrowing its
/// `LocalhostError`. The core throws nothing else.
func core<T>(_ body: () throws -> T) throws(LocalhostError) -> T {
    do {
        return try body()
    } catch let error as LocalhostError {
        throw error
    } catch {
        preconditionFailure("The core threw \(error), which is not a LocalhostError")
    }
}

func core<T>(_ body: () async throws -> T) async throws(LocalhostError) -> T {
    do {
        return try await body()
    } catch let error as LocalhostError {
        throw error
    } catch {
        preconditionFailure("The core threw \(error), which is not a LocalhostError")
    }
}

// MARK: Streams

/// An `AsyncStream` draining one of the core's subscriptions. Ending the
/// stream (or dropping it) closes the subscription.
func makeStream<Element: Sendable>(
    next: @escaping @Sendable () async -> Element?,
    close: @escaping @Sendable () -> Void
) -> AsyncStream<Element> {
    AsyncStream { continuation in
        let task = Task {
            while let element = await next() {
                continuation.yield(element)
            }
            continuation.finish()
        }
        continuation.onTermination = { _ in
            close()
            task.cancel()
        }
    }
}

extension LibraryEvents {
    var stream: AsyncStream<ProjectLibraryEvent> { makeStream(next: next, close: close) }
}

extension ServiceEvents {
    var stream: AsyncStream<ServiceEvent> { makeStream(next: next, close: close) }
}

extension OutputLines {
    var stream: AsyncStream<OutputLine> { makeStream(next: next, close: close) }
}

extension ServiceOutputLines {
    var stream: AsyncStream<ServiceOutputLine> { makeStream(next: next, close: close) }
}

// MARK: Foundation types

extension URL {
    /// A file URL for a path the core returned.
    init(corePath path: String, isDirectory: Bool = false) {
        self.init(filePath: path, directoryHint: isDirectory ? .isDirectory : .notDirectory)
    }
}
