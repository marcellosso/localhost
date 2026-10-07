/// A bounded, in-memory buffer of a Run's most recent output lines.
public struct LogBuffer: Sendable {
    public let capacity: Int
    /// How many lines were dropped to stay within `capacity`.
    public private(set) var droppedCount = 0

    private var storage: [OutputLine] = []
    /// Index of the oldest line once `storage` is full.
    private var head = 0

    public init(capacity: Int) {
        precondition(capacity > 0, "LogBuffer capacity must be positive")
        self.capacity = capacity
        storage.reserveCapacity(min(capacity, 1024))
    }

    public var count: Int { storage.count }

    /// All buffered lines, oldest first.
    public var lines: [OutputLine] {
        Array(storage[head...] + storage[..<head])
    }

    /// The last `limit` lines, oldest first.
    public func recentLines(_ limit: Int) -> [OutputLine] {
        Array(lines.suffix(limit))
    }

    public mutating func append(_ line: OutputLine) {
        if storage.count < capacity {
            storage.append(line)
        } else {
            storage[head] = line
            head = (head + 1) % capacity
            droppedCount += 1
        }
    }
}
