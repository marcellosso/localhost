import Darwin
import Foundation
import Synchronization

/// When a process started, as the kernel reports it. Together with a pid it
/// identifies one process, even after the pid has been reused.
struct ProcessStartTime: Sendable, Hashable, Codable {
    let seconds: UInt64
    let microseconds: UInt64

    static func of(_ pid: pid_t) -> ProcessStartTime? {
        var info = proc_bsdinfo()
        let size = Int32(MemoryLayout<proc_bsdinfo>.size)
        guard proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, size) == size else { return nil }
        return ProcessStartTime(seconds: info.pbi_start_tvsec, microseconds: info.pbi_start_tvusec)
    }
}

/// A process tree left over from a Run whose localhost session ended
/// unexpectedly (crash, force-quit). Orphans are never re-adopted as Running:
/// they can only be stopped or left alone.
public struct Orphan: Sendable, Hashable, Identifiable {
    public var id: UUID { runID }
    public let runID: UUID
    public let serviceID: UUID
    /// The Run's process group (also the pid of its shell).
    public let processGroupID: pid_t
    public let startedAt: Date
    let startTime: ProcessStartTime
}

/// Records every active Run on disk, one file per Run at
/// `<root>/runs/<run-id>.json`, so that a later session can find its Orphans.
///
/// Call `orphans()` at launch: Runs recorded by this session are never reported.
public final class RunRegistry: Sendable {
    static let schemaVersion = 1

    private let directory: URL
    private let sessionRunIDs = Mutex<Set<UUID>>([])

    public init(root: URL = ProjectLibrary.defaultStorageRoot) {
        directory = root.appending(path: "runs", directoryHint: .isDirectory)
    }

    // MARK: Recording (by ServiceManager)

    func record(_ run: Run, pid: pid_t, startTime: ProcessStartTime) {
        sessionRunIDs.withLock { _ = $0.insert(run.id) }
        let record = Record(
            schemaVersion: Self.schemaVersion, runID: run.id, serviceID: run.serviceID,
            pid: pid, startTime: startTime, startedAt: run.startedAt
        )
        // A missing record only means this Run can't be offered as an Orphan
        // after a crash; it must never stop the Run itself.
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try? JSONEncoder().encode(record).write(to: fileURL(run.id), options: .atomic)
    }

    func remove(_ runID: UUID) {
        try? FileManager.default.removeItem(at: fileURL(runID))
        sessionRunIDs.withLock { _ = $0.remove(runID) }
    }

    // MARK: Orphans

    /// Runs recorded by earlier sessions whose process trees are still alive,
    /// oldest first. Records of trees that are gone are deleted.
    public func orphans() -> [Orphan] {
        let sessionRunIDs = sessionRunIDs.withLock { $0 }
        let files = (try? FileManager.default.contentsOfDirectory(atPath: directory.path)) ?? []
        var orphans: [Orphan] = []
        for name in files where name.hasSuffix(".json") {
            let url = directory.appending(path: name, directoryHint: .notDirectory)
            guard let data = try? Data(contentsOf: url),
                  let record = try? JSONDecoder().decode(Record.self, from: data)
            else {
                try? FileManager.default.removeItem(at: url)
                continue
            }
            if sessionRunIDs.contains(record.runID) { continue }
            let orphan = Orphan(
                runID: record.runID, serviceID: record.serviceID, processGroupID: record.pid,
                startedAt: record.startedAt, startTime: record.startTime
            )
            if Self.isAlive(orphan) {
                orphans.append(orphan)
            } else {
                try? FileManager.default.removeItem(at: url)
            }
        }
        return orphans.sorted { $0.startedAt < $1.startedAt }
    }

    /// Stops the Orphans' process groups in parallel (SIGINT, then SIGTERM,
    /// then SIGKILL) and forgets them.
    public func stopOrphans(_ orphans: [Orphan], policy: StopPolicy = .default) async {
        await withDiscardingTaskGroup { group in
            for orphan in orphans {
                group.addTask {
                    await Self.stop(orphan, policy: policy)
                    self.remove(orphan.runID)
                }
            }
        }
    }

    /// Forgets the Orphans, leaving their processes running.
    public func leaveOrphans(_ orphans: [Orphan]) {
        for orphan in orphans { remove(orphan.runID) }
    }

    // MARK: Private

    private func fileURL(_ runID: UUID) -> URL {
        directory.appending(path: "\(runID.uuidString).json", directoryHint: .notDirectory)
    }

    /// Whether the Orphan's process group still exists and is still the one
    /// localhost launched. While a group exists its id can't be reused as a
    /// pid, so if the leader is gone the group is still ours; if the leader is
    /// alive, its start time must match the recorded one.
    private static func isAlive(_ orphan: Orphan) -> Bool {
        guard kill(-orphan.processGroupID, 0) == 0 || errno == EPERM else { return false }
        guard let leaderStartTime = ProcessStartTime.of(orphan.processGroupID) else { return true }
        return leaderStartTime == orphan.startTime
    }

    private static func stop(_ orphan: Orphan, policy: StopPolicy) async {
        let steps: [(Int32, Duration)] = [
            (SIGINT, policy.interruptTimeout),
            (SIGTERM, policy.terminateTimeout),
            (SIGKILL, .seconds(5)),
        ]
        for (signal, timeout) in steps {
            guard isAlive(orphan) else { return }
            kill(-orphan.processGroupID, signal)
            let deadline = ContinuousClock.now + timeout
            while isAlive(orphan), ContinuousClock.now < deadline {
                try? await Task.sleep(for: .milliseconds(20))
            }
        }
    }

    private struct Record: Codable {
        let schemaVersion: Int
        let runID: UUID
        let serviceID: UUID
        let pid: pid_t
        let startTime: ProcessStartTime
        let startedAt: Date
    }
}
