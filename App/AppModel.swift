import Foundation
import LocalhostCore
import Observation

/// The app's single owner of the core. Views read it; they never own Runs.
@Observable
final class AppModel {
    let library: ProjectLibrary?
    let services: ServiceManager?
    let statuses: ServiceStatusStore?
    /// Why the core could not start, e.g. an unreadable Project file.
    let startupError: LocalhostError?

    /// Orphans found at launch, waiting for the user to stop or leave them.
    private(set) var orphans: [Orphan] = []
    private(set) var orphanDescriptions: [String] = []
    private(set) var isStoppingOrphans = false
    var isShowingOrphanAlert = false

    @ObservationIgnored private let runRegistry = RunRegistry()

    init() {
        do {
            let library = try ProjectLibrary()
            let services = ServiceManager(library: library, logs: LogStore(), runRegistry: runRegistry)
            self.library = library
            self.services = services
            statuses = ServiceStatusStore(manager: services)
            startupError = nil
        } catch {
            library = nil
            services = nil
            statuses = nil
            startupError = error
        }
    }

    // MARK: Orphans

    var orphanAlertTitle: String {
        orphans.count == 1
            ? "1 service from the last session is still running"
            : "\(orphans.count) services from the last session are still running"
    }

    /// Looks for Orphans of a previous session. Call once at launch, before
    /// anything is started.
    func checkForOrphans() async {
        let orphans = runRegistry.orphans()
        guard !orphans.isEmpty else { return }
        var descriptions: [String] = []
        for orphan in orphans {
            if let (project, service) = try? await library?.service(id: orphan.serviceID) {
                descriptions.append("\(project.name) › \(service.name)")
            } else {
                descriptions.append("A removed service (process group \(orphan.processGroupID))")
            }
        }
        self.orphans = orphans
        orphanDescriptions = descriptions
        isShowingOrphanAlert = true
    }

    func stopOrphans() async {
        isStoppingOrphans = true
        await runRegistry.stopOrphans(orphans)
        orphans = []
        isStoppingOrphans = false
    }

    func leaveOrphans() {
        runRegistry.leaveOrphans(orphans)
        orphans = []
    }
}
