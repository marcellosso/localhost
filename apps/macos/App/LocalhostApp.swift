import AppKit
import LocalhostCore
import SwiftUI

/// Started from `main.swift`, which may run the shared core instead.
struct LocalhostApp: App {
    @NSApplicationDelegateAdaptor private var delegate: AppDelegate

    var body: some Scene {
        Window("localhost", id: "main") {
            MainView()
                .environment(delegate.model)
                .task { await delegate.model.runLaunchChecks() }
                // The Dock icon shows only while the main window is open.
                .onAppear { NSApp.setActivationPolicy(.regular) }
                .onDisappear { NSApp.setActivationPolicy(.accessory) }
        }
        .defaultSize(width: 1280, height: 800)
        .commands {
            AppCommands(model: delegate.model)
            NavigationCommands(model: delegate.model)
        }
        MenuBarExtra {
            MenuBarContent()
                .environment(delegate.model)
        } label: {
            Image(nsImage: MenuBarIcon.image(hasFailure: delegate.model.hasFailedService))
                .accessibilityLabel(delegate.model.hasFailedService ? "localhost, a service failed" : "localhost")
        }
        .menuBarExtraStyle(.window)
        Window("Settings", id: "settings") {
            SettingsView()
                .environment(delegate.model)
        }
        .windowStyle(.hiddenTitleBar)
        .windowResizability(.contentSize)
        .defaultPosition(.center)
        .commandsRemoved()
    }
}

/// Closing the window keeps the app (and its Services) running. Quitting with
/// active Services asks first, then stops them all in parallel.
final class AppDelegate: NSObject, NSApplicationDelegate {
    let model = AppModel()

    func applicationWillFinishLaunching(_ notification: Notification) {
        Preferences.applyAppearance()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        let origin = model.quitOrigin
        model.quitOrigin = .mainWindow
        let count = model.activeServiceCount
        guard count > 0 else { return .terminateNow }

        // Without a main window the app may be in the background (no Dock icon).
        NSApp.activate()
        let alert = NSAlert()
        alert.messageText = count == 1 ? "Stop 1 service and quit?" : "Stop \(count) services and quit?"
        alert.informativeText = "Running services are stopped before localhost quits."
        alert.addButton(withTitle: "Stop and Quit")
        alert.addButton(withTitle: "Cancel")
        guard alert.runModal() == .alertFirstButtonReturn else { return .terminateCancel }

        Task {
            await model.stopAllServices(origin: origin)
            sender.reply(toApplicationShouldTerminate: true)
        }
        return .terminateLater
    }
}

struct AppCommands: Commands {
    let model: AppModel
    @Environment(\.openWindow) private var openWindow

    var body: some Commands {
        CommandGroup(after: .appInfo) {
            Button("Check for Updates…") { model.updates.checkForUpdates() }
        }
        CommandGroup(replacing: .appSettings) {
            Button("Settings…") { openWindow(id: "settings") }
                .keyboardShortcut(",")
        }
        CommandGroup(replacing: .help) {
            Button("Keyboard Shortcuts") {
                openWindow(id: "main")
                model.toggleKeyboardShortcuts()
            }
            .keyboardShortcut("/")
        }
        CommandGroup(before: .windowList) {
            Button("Main Window") { openWindow(id: "main") }
            Button("Ports") {
                openWindow(id: "main")
                model.sidebarSelection = .ports
            }
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
            Button("Detect Services…") {
                if let project = model.selectedProject { model.presentedSheet = .detectServices(projectID: project.id) }
            }
            .disabled(model.selectedProject == nil)
            Divider()
            Button("Import Project…") {
                openWindow(id: "main")
                model.isChoosingImportFile = true
            }
            .disabled(model.core == nil)
            Button("Export Project…") {
                openWindow(id: "main")
                if let project = model.selectedProject { model.exportProject(project.id) }
            }
            .disabled(model.selectedProject == nil)
        }
        CommandGroup(replacing: .textEditing) {
            Button("Find in Log") { model.findInLog() }
                .keyboardShortcut("f")
                .disabled(!model.canClearLogs)
            Button("Clear Log") { model.clearLogs() }
                .keyboardShortcut("l", modifiers: .control)
                .disabled(!model.canClearLogs)
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
            // In Ports, Stop applies to the selected Listener.
            let listener = model.sidebarSelection == .ports ? model.selectedListenerID.flatMap(model.listener) : nil
            Button(listener.map { "Stop \($0.name)" + (model.service(of: $0) == nil ? "…" : "") } ?? "Stop") {
                if let listener { model.requestStop(listener) } else { model.selectedServiceID.map { model.stop($0) } }
            }
            .keyboardShortcut(".")
            .disabled(listener.map { model.reasonNotStoppable($0) != nil } ?? !isLive)
            Divider()
            Button("Open URL") { model.selectedServiceID.map { model.openURL(of: $0) } }
                .keyboardShortcut("o", modifiers: [.command, .shift])
                .disabled(!isLive || model.selectedService.flatMap(model.urlToOpen) == nil)
            Divider()
            Button("Edit Service…") { model.selectedService.map { model.presentedSheet = .editService($0.id) } }
                .disabled(model.selectedService == nil)
            Button("Remove Service…") { model.selectedService.map { model.pendingRemoval = .service($0.id) } }
                .disabled(model.selectedService == nil)
        }
        CommandMenu("Project") {
            let project = model.selectedProject
            Button("Start All") { project.map { model.startAll(inProject: $0.id) } }
                .keyboardShortcut("r", modifiers: [.command, .shift])
                .disabled(project?.services.isEmpty ?? true)
            Button("Stop All") { project.map { model.stopAll(inProject: $0.id) } }
                .keyboardShortcut(".", modifiers: [.command, .shift])
                .disabled(!(project.map { $0.services.contains { model.status(of: $0).state.isActive } } ?? false))
            Divider()
            Button("Edit Project…") { project.map { model.presentedSheet = .editProject($0.id) } }
                .disabled(project == nil)
            Button("Edit Environment…") { project.map { model.presentedSheet = .projectEnvironment($0.id) } }
                .disabled(project == nil)
            Button(project?.isFavorite == true ? "Unfavorite" : "Favorite") {
                project.map { model.setFavorite($0.id, !$0.isFavorite) }
            }
            .disabled(project == nil)
            Divider()
            FolderActionCommands(model: model)
            Divider()
            Button("Remove Project…") { project.map { model.pendingRemoval = .project($0.id) } }
                .disabled(project == nil)
        }
    }
}
