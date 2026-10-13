import Foundation

// A record of every live Run on disk, so processes left behind by a crash or
// a force-quit (Orphans) are found at the next launch.

extension RunRegistry {
    public convenience init(root: URL = ProjectLibrary.defaultStorageRoot) {
        self.init(unsafeFromHandle: RunRegistry.ffiNew(root: root.path(percentEncoded: false)).uniffiCloneHandle())
    }

    /// Recorded Runs whose process group is still alive.
    public func orphans() -> [Orphan] {
        ffiOrphans()
    }

    /// Stops each Orphan's process group (SIGINT → SIGTERM → SIGKILL).
    public func stopOrphans(_ orphans: [Orphan]) async {
        await ffiStopOrphans(orphans: orphans)
    }

    /// Forgets the Orphans, leaving their processes running.
    public func leaveOrphans(_ orphans: [Orphan]) {
        ffiLeaveOrphans(orphans: orphans)
    }
}
