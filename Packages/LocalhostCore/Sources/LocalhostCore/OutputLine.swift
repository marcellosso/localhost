import Foundation

/// One line of a Run's output.
public struct OutputLine: Sendable, Hashable {
    public enum Channel: String, Sendable, Hashable, Codable {
        case stdout
        case stderr
    }

    public let channel: Channel
    /// The line without its terminator. Invalid UTF-8 is replaced with U+FFFD.
    public let text: String
    public let timestamp: Date
}
