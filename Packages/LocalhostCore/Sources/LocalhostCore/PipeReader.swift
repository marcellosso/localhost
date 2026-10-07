import Dispatch
import Foundation

/// Reads one pipe and splits it into lines.
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

    /// Starts reading `fileDescriptor`, which the reader owns and closes. The
    /// reader keeps itself alive until the pipe ends or `cancel()` is called.
    /// `onLine` and `onEnd` are called on the reader's serial queue.
    init(
        fileDescriptor: Int32,
        channel: OutputLine.Channel,
        onLine: @escaping @Sendable (OutputLine) -> Void,
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
            readAvailable(from: fileDescriptor, onLine: onLine)
        }
        source.setCancelHandler { [self] in
            if !buffer.isEmpty { onLine(makeLine(buffer)) }
            buffer = []
            close(fileDescriptor)
            onEnd()
        }
        source.resume()
    }

    /// Stops reading, delivering any partial line. Safe to call more than once.
    func cancel() {
        source.cancel()
    }

    private func readAvailable(from fileDescriptor: Int32, onLine: (OutputLine) -> Void) {
        var chunk = [UInt8](repeating: 0, count: 64 * 1024)
        let count = read(fileDescriptor, &chunk, chunk.count)
        if count < 0 {
            if errno == EAGAIN || errno == EINTR { return }
            source.cancel()
            return
        }
        if count == 0 {
            source.cancel()
            return
        }
        buffer.append(contentsOf: chunk[..<count])
        while let newline = buffer.firstIndex(of: UInt8(ascii: "\n")) {
            onLine(makeLine(buffer[..<newline]))
            buffer.removeSubrange(...newline)
        }
        while buffer.count >= Self.maxLineLength {
            onLine(makeLine(buffer[..<Self.maxLineLength]))
            buffer.removeSubrange(..<Self.maxLineLength)
        }
    }

    private func makeLine(_ bytes: some Collection<UInt8>) -> OutputLine {
        var text = String(decoding: bytes, as: UTF8.self)
        if text.hasSuffix("\r") { text.removeLast() }
        return OutputLine(channel: channel, text: text, timestamp: Date())
    }
}
