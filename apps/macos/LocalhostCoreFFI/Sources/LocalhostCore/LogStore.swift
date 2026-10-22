import Foundation

// Each Run's output: a ring buffer in memory and a file under logs/.

extension LogStore {
    public static var defaultMaxLinesPerRun: Int { Int(logStoreDefaultMaxLinesPerRun()) }

    public convenience init(root: URL = ProjectLibrary.defaultStorageRoot) {
        self.init(unsafeFromHandle: LogStore.ffiNew(root: root.path(percentEncoded: false)).uniffiCloneHandle())
    }

    public func lines(forRun runID: UUID) -> [OutputLine] {
        ffiLines(runId: runID)
    }

    public func recentLines(forRun runID: UUID, limit: Int) -> [OutputLine] {
        ffiRecentLines(runId: runID, limit: UInt64(max(limit, 0)))
    }

    /// The last lines a Service printed itself, without localhost's own.
    public func recentOutput(forRun runID: UUID, limit: Int) -> [OutputLine] {
        ffiRecentOutput(runId: runID, limit: UInt64(max(limit, 0)))
    }

    public func latestRunID(ofService serviceID: UUID) -> UUID? {
        ffiLatestRunId(serviceId: serviceID)
    }

    /// The Run's buffered lines, and a stream of every line after them.
    public func subscribe(toRun runID: UUID) -> (lines: [OutputLine], updates: AsyncStream<OutputLine>) {
        let subscription = ffiSubscribeToRun(runId: runID)
        return (subscription.lines, subscription.updates.stream)
    }

    /// The Services' latest Runs merged by time, and a stream of every line
    /// any of their Runs prints after that, including Runs that begin later.
    public func subscribe(
        toServices serviceIDs: Set<UUID>
    ) -> (lines: [ServiceOutputLine], updates: AsyncStream<ServiceOutputLine>) {
        let subscription = ffiSubscribeToServices(serviceIds: Array(serviceIDs))
        return (subscription.lines, subscription.updates.stream)
    }

    /// Empties the latest Run's buffer, so viewers start from new output.
    public func clearBuffer(ofService serviceID: UUID) {
        ffiClearBuffer(serviceId: serviceID)
    }

    public func logFileURL(forRun runID: UUID) -> URL? {
        ffiLogFilePath(runId: runID).map { URL(corePath: $0) }
    }

    public func removeLogs(ofService serviceID: UUID) async {
        await ffiRemoveLogs(serviceId: serviceID)
    }
}
