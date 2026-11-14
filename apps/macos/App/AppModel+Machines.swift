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
        shown.state.summary(machine: shown.machine.name)
    }

    /// Whether the Machine is waiting for the user to connect it again.
    func needsReconnect(_ shown: MachinesStore.Shown) -> Bool {
        shown.state.needsUser
    }
}

// MARK: Its Projects

// A Machine's Projects are changed there, through its connection, and come
// back as it reports them: nothing here edits a copy, and nothing waits for
// a Machine that isn't connected.
extension AppModel {
    /// The Machines, to make a change on `machineID`.
    func machines(for machineID: UUID) throws(LocalhostError) -> Machines {
        guard let machines else { throw machinesError ?? .MachineNotFound(machineID) }
        return machines
    }

    /// The name of the Machine `place` is, or `nil` for this Mac.
    func machineName(_ place: Place) -> String? {
        guard case .machine(let machineID) = place else { return nil }
        return shownMachine(machineID)?.machine.name ?? "its Machine"
    }

    /// Why the Projects of `place` can't be changed now, or `nil` when they
    /// can: another Machine while it isn't connected says how it is, and one
    /// with an older localhost says to update it.
    func reasonCantChange(_ place: Place) -> String? {
        switch place {
        case .thisMac:
            return core == nil ? startupError?.message : nil
        case .machine(let machineID):
            guard let shown = shownMachine(machineID) else { return LocalhostError.MachineNotFound(machineID).message }
            guard shown.isConnected else { return stateDescription(of: shown) }
            return shown.managesProjects ? nil : LocalhostError.MachineNeedsUpdate(machine: shown.machine.name).message
        }
    }

    /// Whether Projects can be added to `place`, and its Projects changed, now.
    func canChange(_ place: Place) -> Bool {
        place == .thisMac ? core != nil : reasonCantChange(place) == nil
    }

    /// The note a Machine's Projects show while its localhost is too old to
    /// change them; `nil` elsewhere.
    func needsUpdateNote(_ place: Place) -> String? {
        guard case .machine(let machineID) = place, let shown = shownMachine(machineID),
              shown.isConnected, !shown.managesProjects
        else { return nil }
        return LocalhostError.MachineNeedsUpdate(machine: shown.machine.name).message
    }

    /// A folder on the Machine, for its folder picker; `nil` is its home folder.
    func listDirectory(
        _ path: String?, on machineID: UUID, showHidden: Bool, includeFiles: Bool
    ) async throws(LocalhostError) -> DirectoryListing {
        try await machines(for: machineID).listDirectory(path, on: machineID, showHidden: showHidden, includeFiles: includeFiles)
    }

    /// Import Project… for `place`: the open panel on this Mac, the
    /// Machine's own file picker on another.
    func chooseProjectFile(on place: Place) {
        switch place {
        case .thisMac: isChoosingImportFile = true
        case .machine(let machineID): presentedSheet = .chooseProjectFile(machine: machineID)
        }
    }

    /// Reads a Project file on the Machine and opens its review sheet, or
    /// says why it can't be imported.
    func openProjectFile(at path: String, on machineID: UUID) async {
        let fileName = path.split(separator: "/").last.map(String.init) ?? path
        do throws(LocalhostError) {
            let file = try await machines(for: machineID).readProjectFile(at: path, on: machineID)
            presentedSheet = .importProject(
                try ProjectDocument(yaml: file.text), fileName: fileName, on: .machine(machineID), folderExists: file.folderExists
            )
        } catch {
            importFailure = ImportFailure(fileName: fileName, error: error)
        }
    }
}
