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
            let isLive = status?.state == .starting || status?.state == .running
            Button(isLive ? "Restart" : "Start") {
                guard let id = model.selectedServiceID else { return }
                isLive ? model.restart(id) : model.start(id)
            }
            .keyboardShortcut("r")
            .disabled(status == nil || status?.state == .stopping)
            Button("Stop") { model.selectedServiceID.map(model.stop) }
                .keyboardShortcut(".")
                .disabled(!isLive)
            Divider()
            Button("Open URL") { model.selectedServiceID.map { model.openURL(of: $0) } }
                .keyboardShortcut("o", modifiers: [.command, .shift])
                .disabled(!isLive || model.selectedService.flatMap(model.urlToOpen) == nil)
        }
        CommandMenu("Project") {
            let project = model.selectedProject
            Button("Start All") { project.map { model.startAll(inProject: $0.id) } }
                .keyboardShortcut("r", modifiers: [.command, .shift])
                .disabled(project?.services.isEmpty ?? true)
            Button("Stop All") { project.map { model.stopAll(inProject: $0.id) } }
                .keyboardShortcut(".", modifiers: [.command, .shift])
                .disabled(!(project.map { $0.services.contains { model.status(of: $0).state.isActive } } ?? false))
        }
    }
}
