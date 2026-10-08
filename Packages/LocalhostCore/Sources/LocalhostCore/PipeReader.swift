import Dispatch
import Foundation
import Synchronization

/// Reads one pipe and splits it into lines, delivered a read at a time.
///
/// All mutable state is confined to `queue`, which is why this class can be
/// `@unchecked Sendable`.
final class PipeReader: @unchecked Sendable {
    /// A line longer than this is delivered in pieces, so output without
    /// newlines (progress bars) can't grow the buffer without bound.
    static let maxLineLength = 64 * 1024

    private let channel: OutputLine.Channel
    private let source: DispatchSourceRead
    private var buffer: [UInt8] = []
    /// A suspended source must be resumed before it is cancelled, or its
    /// cancel handler never runs.
    private let suspension = Mutex<(isSuspended: Bool, isCancelled: Bool)>((false, false))

    /// Starts reading `fileDescriptor`, which the reader owns and closes. The
    /// reader keeps itself alive until the pipe ends or `cancel()` is called.
    /// `onLines` (never with an empty array) and `onEnd` are called on the
    /// reader's serial queue. Reading pauses while `flow` has too many
    /// batches waiting.
    init(
        fileDescriptor: Int32,
        channel: OutputLine.Channel,
        flow: OutputFlow,
        onLines: @escaping @Sendable ([OutputLine]) -> Void,
        onEnd: @escaping @Sendable () -> Void
    ) {
        self.channel = channel
        let flags = fcntl(fileDescriptor, F_GETFL)
        _ = fcntl(fileDescriptor, F_SETFL, flags | O_NONBLOCK)
        source = DispatchSource.makeReadSource(
            fileDescriptor: fileDescriptor,
            queue: DispatchQueue(label: "localhost.pipe-reader.\(channel.rawValue)")
        )
        source.setEventHandler { [self] in
            let lines = readAvailable(from: fileDescriptor)
            if !lines.isEmpty {
                onLines(lines)
                flow.produced(by: self)
            }
        }
        source.setCancelHandler { [self] in
            if !buffer.isEmpty { onLines([makeLine(buffer)]) }
            buffer = []
            close(fileDescriptor)
            onEnd()
        }
        source.resume()
    }

    /// Stops reading, delivering any partial line. Safe to call more than once.
    func cancel() {
        suspension.withLock { state in
            state.isCancelled = true
            if state.isSuspended {
                source.resume()
                state.isSuspended = false
            }
        }
        source.cancel()
    }

    /// Stops reading until `resume()`; the writer blocks once the pipe is full.
    func pause() {
        suspension.withLock { state in
            guard !state.isSuspended, !state.isCancelled else { return }
            source.suspend()
            state.isSuspended = true
        }
    }

    func resume() {
        suspension.withLock { state in
            guard state.isSuspended else { return }
            source.resume()
            state.isSuspended = false
        }
    }

    private func readAvailable(from fileDescriptor: Int32) -> [OutputLine] {
        var chunk = [UInt8](repeating: 0, count: 64 * 1024)
        let count = read(fileDescriptor, &chunk, chunk.count)
        if count < 0 {
            if errno == EAGAIN || errno == EINTR { return [] }
            cancel()
            return []
        }
        if count == 0 {
            cancel()
            return []
        }
        buffer.append(contentsOf: chunk[..<count])
        var lines: [OutputLine] = []
        var start = buffer.startIndex
        while let newline = buffer[start...].firstIndex(of: UInt8(ascii: "\n")) {
            lines.append(makeLine(buffer[start..<newline]))
            start = newline + 1
        }
        buffer.removeSubrange(..<start)
        while buffer.count >= Self.maxLineLength {
            lines.append(makeLine(buffer[..<Self.maxLineLength]))
            buffer.removeSubrange(..<Self.maxLineLength)
        }
        return lines
    }

    private func makeLine(_ bytes: some Collection<UInt8>) -> OutputLine {
        var text = String(decoding: bytes, as: UTF8.self)
        if text.hasSuffix("\r") { text.removeLast() }
        return OutputLine(channel: channel, text: text, timestamp: Date())
    }
}

/// Counts the output batches of a process tree that were read but not yet
/// consumed, pausing its readers while too many wait. A command that prints
/// faster than localhost can keep up then blocks on its pipe, as it would in
/// a terminal, instead of growing localhost's memory.
final class OutputFlow: Sendable {
    /// About 1 MB of output at 64 KB per read.
    static let pauseThreshold = 16
    static let resumeThreshold = 4

    private let state = Mutex<(pending: Int, paused: [PipeReader])>((0, []))

    func produced(by reader: PipeReader) {
        state.withLock { state in
            state.pending += 1
            if state.pending >= Self.pauseThreshold {
                reader.pause()
                state.paused.append(reader)
            }
        }
    }

    func consumed() {
        state.withLock { state in
            state.pending -= 1
            guard state.pending <= Self.resumeThreshold else { return }
            for reader in state.paused { reader.resume() }
            state.paused = []
        }
    }
}

/// A process tree's output lines, in batches as read from its pipes. Taking
/// a batch lets reading go on (see `OutputFlow`).
public struct OutputBatches: AsyncSequence, Sendable {
    public typealias Element = [OutputLine]

    let stream: AsyncStream<[OutputLine]>
    let flow: OutputFlow

    public func makeAsyncIterator() -> AsyncIterator {
        AsyncIterator(base: stream.makeAsyncIterator(), flow: flow)
    }

    public struct AsyncIterator: AsyncIteratorProtocol {
        var base: AsyncStream<[OutputLine]>.AsyncIterator
        let flow: OutputFlow

        public mutating func next() async -> [OutputLine]? {
            let batch = await base.next()
            if batch != nil { flow.consumed() }
            return batch
        }
    }
}
