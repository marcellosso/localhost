import Foundation

/// One line of a Run's output.
public struct OutputLine: Sendable, Hashable {
    public enum Channel: String, Sendable, Hashable, Codable {
        case stdout
        case stderr
        /// Written by localhost itself, e.g. how the Run ended.
        case localhost
    }

    public let channel: Channel
    /// The line without its terminator. Invalid UTF-8 is replaced with U+FFFD.
    public let text: String
    public let timestamp: Date
}

/// A line of one Service's output, in a stream that combines several Services.
public struct ServiceOutputLine: Sendable, Hashable {
    public let serviceID: UUID
    public let line: OutputLine
}
