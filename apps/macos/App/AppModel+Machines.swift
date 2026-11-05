import AppKit
import LocalhostCore
import SwiftUI

// Other Machines: Settings › Machines adds, renames and removes them; the
// sidebar shows each one's Projects below this Mac's. The core keeps each
// connected and reconnects after a loss; it never starts or stops anything
// on its own.

extension AppModel {
    /// The Machines as shown, in the order they were added.
    var shownMachines: [MachinesStore.Shown] {
        machineStore?.machines ?? []
    }

    func shownMachine(_ machineID: UUID) -> MachinesStore.Shown? {
        machineStore?.machine(machineID)
    }

    /// Active Services on connected Machines started from a Machine: those
    /// this Mac may hold, which stop there once it disconnects, unless
    /// localhost is open on that Machine.
    var activeMachineServiceCount: Int {
        shownMachines.filter(\.isConnected).reduce(0) { count, shown in
            count + shown.statuses.values.filter { $0.state.isActive && $0.run?.origin == .machine }.count
        }
    }

    /// Connects to every Machine at launch, and tries those waiting to
    /// reconnect again as soon as the Mac wakes.
    func connectToMachines() {
        guard let machines else { return }
        for machine in machines.machines() {
            try? machines.connect(machine.id)
        }
        _ = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didWakeNotification, object: nil, queue: .main
        ) { _ in
            machines.wake()
        }
    }

    /// Has VoiceOver say when a Service of another Machine settles, as
    /// `announceStates` does here: with its Machine and Project unless they
    /// are the selected ones; Failed interrupts.
    func announceMachineStates() {
        guard let machines else { return }
        Task { [weak self] in
            var lastStates: [String: ServiceState] = [:]
            for await event in machines.subscribe().events {
                guard let self, case .serviceChanged(let machineID, .serviceStateChanged(let status)) = event else { continue }
                let key = "\(machineID)/\(status.serviceID)"
                defer { lastStates[key] = status.state }
                guard status.state != lastStates[key], !status.state.isActive || status.state == .running,
                      NSWorkspace.shared.isVoiceOverEnabled,
                      let (project, service) = self.service(status.serviceID, on: .machine(machineID)),
                      let shown = self.shownMachine(machineID)
                else { continue }
                let isSelected = self.selectedPlace == .machine(machineID) && project.id == self.selectedProjectID
                let subject = isSelected ? service.name : "\(shown.machine.name), \(project.name), \(service.name)"
                var announcement = AttributedString("\(subject), \(status.spokenSummary)")
                announcement.accessibilitySpeechAnnouncementPriority = status.state == .failed ? .high : .default
                AccessibilityNotification.Announcement(announcement).post()
            }
        }
    }

    /// Connects to the Machine first, and adds it only once that works.
    @discardableResult
    func addMachine(destination: String, name: String) async throws(LocalhostError) -> Machine {
        guard let machines else { throw machinesError ?? .MachineNotFound(UUID()) }
        let name = name.trimmingCharacters(in: .whitespaces)
        return try await machines.add(destination: destination, name: name.isEmpty ? nil : name)
    }

    func renameMachine(_ machineID: UUID, to name: String) {
        guard let machines else { return }
        Task { await report { () throws(LocalhostError) in try await machines.rename(machineID, to: name) } }
    }

    /// Disconnects and forgets the Machine; nothing changes on it.
    func removeMachine(_ machineID: UUID) {
        guard let machines else { return }
        if selectedPlace == .machine(machineID) { sidebarSelection = nil }
        Task { await report { () throws(LocalhostError) in try await machines.remove(machineID) } }
    }

    /// Connects again now: at once if it was waiting, else anew.
    func reconnectMachine(_ machineID: UUID) {
        guard let machines else { return }
        do throws(LocalhostError) {
            if shownMachine(machineID)?.isConnected == true {
                try machines.reconnect(machineID)
            } else {
                try machines.connect(machineID)
            }
        } catch {
            actionError = error
        }
    }

    /// "Connected · localhost 0.5.0", "Connecting…", or why it isn't.
    func stateDescription(of shown: MachinesStore.Shown) -> String {
        switch shown.state {
        case .notConnected: "Not connected"
        case .connecting: "Connecting…"
        case .connected(let version): "Connected · localhost \(version)"
        case .unavailable(let problem, let isRetrying):
            problem.message(machine: shown.machine.name) + (isRetrying ? " Trying again…" : "")
        }
    }

    /// Whether the Machine is waiting for the user to connect it again.
    func needsReconnect(_ shown: MachinesStore.Shown) -> Bool {
        switch shown.state {
        case .notConnected: true
        case .unavailable(_, let isRetrying): !isRetrying
        case .connecting, .connected: false
        }
    }
}
