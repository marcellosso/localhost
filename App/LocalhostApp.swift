import AppKit
import LocalhostCore
import SwiftUI

@main
struct LocalhostApp: App {
    @NSApplicationDelegateAdaptor private var delegate: AppDelegate

    var body: some Scene {
        Window("localhost", id: "main") {
            MainView()
                .environment(delegate.model)
                .task { await delegate.model.checkForOrphans() }
        }
        .defaultSize(width: 1100, height: 700)
        .commands {
            AppCommands(model: delegate.model)
        }
    }
}

/// Closing the window keeps the app (and its Services) running. Quitting with
/// active Services asks first, then stops them all in parallel.
final class AppDelegate: NSObject, NSApplicationDelegate {
    let model = AppModel()

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        let count = model.activeServiceCount
        guard count > 0 else { return .terminateNow }

        let alert = NSAlert()
        alert.messageText = count == 1 ? "Stop 1 service and quit?" : "Stop \(count) services and quit?"
        alert.informativeText = "Running services are stopped before localhost quits."
        alert.addButton(withTitle: "Stop and Quit")
        alert.addButton(withTitle: "Cancel")
        guard alert.runModal() == .alertFirstButtonReturn else { return .terminateCancel }

        Task {
            await model.stopAllServices()
            sender.reply(toApplicationShouldTerminate: true)
        }
        return .terminateLater
    }
}

struct AppCommands: Commands {
    let model: AppModel
    @Environment(\.openWindow) private var openWindow

    var body: some Commands {
        CommandGroup(before: .windowList) {
            Button("Main Window") { openWindow(id: "main") }
                .keyboardShortcut("0")
            Divider()
        }
        CommandGroup(replacing: .newItem) {
            Button("New Project…") { model.presentedSheet = .addProject }
                .keyboardShortcut("n")
                .disabled(model.core == nil)
            Button("New Service…") {
                if let project = model.selectedProject { model.presentedSheet = .addService(projectID: project.id) }
            }
            .keyboardShortcut("n", modifiers: [.command, .shift])
            .disabled(model.selectedProject == nil)
        }
        CommandMenu("Service") {
            let status = model.selectedService.map { model.status(of: $0) }
            Button("Start") { model.selectedServiceID.map(model.start) }
                .keyboardShortcut("r")
                .disabled(status == nil || status?.state.isActive == true)
            Button("Stop") { model.selectedServiceID.map(model.stop) }
                .keyboardShortcut(".")
                .disabled(!(status?.state == .starting || status?.state == .running))
            Button("Restart") { model.selectedServiceID.map(model.restart) }
                .keyboardShortcut("r", modifiers: [.command, .shift])
                .disabled(status == nil || status?.state == .stopping)
        }
    }
}
