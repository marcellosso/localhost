import AppKit
import LocalhostCore

/// What the menu bar companion reads and does. It shares the model (and so
/// the core) with the main window; its state-changing calls use `.menuBar`.
extension AppModel {
    /// An Unmanaged Listener holding the Port of a Service that isn't running.
    struct PortWarning: Identifiable, Hashable {
        let port: Int
        let serviceID: UUID
        let holder: Listener

        var id: String { "\(serviceID)-\(port)" }
    }

    /// Projects with any active or Failed Service, plus favourites, in sidebar order.
    var menuBarProjects: [Project] {
        projects.filter { project in
            project.isFavorite || project.services.contains {
                let state = status(of: $0).state
                return state.isActive || state == .failed
            }
        }
    }

    /// Whether any Service is Failed (the red dot on the menu bar icon).
    var hasFailedService: Bool {
        projects.contains { project in project.services.contains { status(of: $0).state == .failed } }
    }

    /// Opens the main window on a Project, and on one of its Services if given.
    func reveal(projectID: UUID, serviceID: UUID? = nil) {
        selectedProjectID = projectID
        if let serviceID { selectedServiceID = serviceID }
        showMainWindow?()
    }

    /// Opens the main window's Ports view on a Listener.
    func reveal(listener: Listener) {
        sidebarSelection = .ports
        selectedListenerID = listener.id
        showMainWindow?()
    }

    /// Quits through the same confirm-and-stop flow as ⌘Q.
    func quit(origin: Origin) {
        quitOrigin = origin
        NSApp.terminate(nil)
    }
}
