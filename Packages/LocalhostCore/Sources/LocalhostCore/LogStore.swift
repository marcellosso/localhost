import Foundation

/// Keeps Run output: a bounded in-memory buffer of each Service's latest Run
/// for the viewer, and a file per Run at `<root>/logs/<service-uuid>/<run-id>.log`.
///
/// Each file line is `<ISO 8601 timestamp> <stdout|stderr|localhost> <text>`,
/// with the text as written (ANSI sequences included). The last
/// `retainedRunsPerService` files are kept per Service; a file rotates into a
/// `.log.1` backup so the pair stays within `maxFileSize`.
public actor LogStore {
    public static let defaultMaxLinesPerRun = 10_000
    public static let defaultMaxFileSize = 10 * 1024 * 1024
    public static let defaultRetainedRunsPerService = 5

    private let logsDirectory: URL
    private let maxLinesPerRun: Int
    private let maxFileSize: Int
    private let retainedRunsPerService: Int

    private var buffers: [UUID: LogBuffer] = [:]
    private var latestRunOfService: [UUID: UUID] = [:]
    private var files: [UUID: LogFile] = [:]
    private var fileURLs: [UUID: URL] = [:]
    private var subscribers: [UUID: [AsyncStream<OutputLine>.Continuation]] = [:]

    public init(
        root: URL = ProjectLibrary.defaultStorageRoot,
        maxLinesPerRun: Int = LogStore.defaultMaxLinesPerRun,
        maxFileSize: Int = LogStore.defaultMaxFileSize,
        retainedRunsPerService: Int = LogStore.defaultRetainedRunsPerService
    ) {
        logsDirectory = root.appending(path: "logs", directoryHint: .isDirectory)
        self.maxLinesPerRun = maxLinesPerRun
        self.maxFileSize = maxFileSize
        self.retainedRunsPerService = retainedRunsPerService
    }

    // MARK: Reading

    /// The buffered lines of a Run, oldest first. Empty unless it is its
    /// Service's latest Run.
    public func lines(forRun runID: UUID) -> [OutputLine] {
        buffers[runID]?.lines ?? []
    }

    /// The last `limit` buffered lines of a Run.
    public func recentLines(forRun runID: UUID, limit: Int) -> [OutputLine] {
        buffers[runID]?.recentLines(limit) ?? []
    }

    public func latestRunID(ofService serviceID: UUID) -> UUID? {
        latestRunOfService[serviceID]
    }

    /// The buffered lines of a Run plus a stream of every line appended after
    /// them. The stream finishes when the Run's output ends.
    public func subscribe(toRun runID: UUID) -> (lines: [OutputLine], updates: AsyncStream<OutputLine>) {
        let (stream, continuation) = AsyncStream.makeStream(of: OutputLine.self)
        if files[runID] == nil {
            continuation.finish()
        } else {
            subscribers[runID, default: []].append(continuation)
        }
        return (lines(forRun: runID), stream)
    }

    public func logFileURL(forRun runID: UUID) -> URL? {
        fileURLs[runID]
    }

    // MARK: Writing (by ServiceManager)

    func beginRun(_ run: Run) {
        if let previous = latestRunOfService[run.serviceID], files[previous] == nil {
            buffers[previous] = nil
        }
        latestRunOfService[run.serviceID] = run.id
        buffers[run.id] = LogBuffer(capacity: maxLinesPerRun)

        let directory = logsDirectory.appending(path: run.serviceID.uuidString, directoryHint: .isDirectory)
        let url = directory.appending(path: "\(run.id.uuidString).log", directoryHint: .notDirectory)
        fileURLs[run.id] = url
        files[run.id] = LogFile(url: url, maxSize: maxFileSize)
        removeOldRuns(in: directory)
    }

    func append(_ line: OutputLine, toRun runID: UUID) {
        guard files[runID] != nil else { return }
        buffers[runID]?.append(line)
        files[runID]?.write(line)
        for subscriber in subscribers[runID] ?? [] {
            subscriber.yield(line)
        }
    }

    func endRun(_ runID: UUID) {
        files[runID]?.close()
        files[runID] = nil
        for subscriber in subscribers.removeValue(forKey: runID) ?? [] {
            subscriber.finish()
        }
    }

    // MARK: Private

    /// Deletes all but the newest `retainedRunsPerService` Run files.
    private func removeOldRuns(in directory: URL) {
        let fileManager = FileManager.default
        guard let urls = try? fileManager.contentsOfDirectory(
            at: directory, includingPropertiesForKeys: [.creationDateKey]
        ) else { return }
        let runFiles = urls
            .filter { $0.pathExtension == "log" }
            .map { url in (url, (try? url.resourceValues(forKeys: [.creationDateKey]).creationDate) ?? .distantPast) }
            .sorted { $0.1 > $1.1 }
        for (url, _) in runFiles.dropFirst(retainedRunsPerService) {
            try? fileManager.removeItem(at: url)
            try? fileManager.removeItem(at: url.appendingPathExtension("1"))
        }
    }
}

/// One Run's log file. Rotates into `<name>.1` when it reaches half of
/// `maxSize`, so the file and its backup never exceed `maxSize` together.
private final class LogFile {
    private let url: URL
    private let segmentSize: Int
    private var handle: FileHandle?
    private var size = 0

    private static let timestampStyle = Date.ISO8601FormatStyle(includingFractionalSeconds: true)

    init(url: URL, maxSize: Int) {
        self.url = url
        segmentSize = maxSize / 2
        try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        open()
    }

    func write(_ line: OutputLine) {
        var data = Data("\(Self.timestampStyle.format(line.timestamp)) \(line.channel.rawValue) \(line.text)\n".utf8)
        if data.count > segmentSize {
            data = data.suffix(segmentSize)
        }
        if size + data.count > segmentSize {
            rotate()
        }
        try? handle?.write(contentsOf: data)
        size += data.count
    }

    func close() {
        try? handle?.close()
        handle = nil
    }

    private func open() {
        FileManager.default.createFile(atPath: url.path, contents: nil)
        handle = try? FileHandle(forWritingTo: url)
        size = 0
    }

    private func rotate() {
        close()
        let backup = url.appendingPathExtension("1")
        try? FileManager.default.removeItem(at: backup)
        try? FileManager.default.moveItem(at: url, to: backup)
        open()
    }
}
