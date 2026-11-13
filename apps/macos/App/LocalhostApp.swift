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
        let count = model.activeServiceCount + model.activeMachineServiceCount
        guard count > 0 else { return .terminateNow }

        // Without a main window the app may be in the background (no Dock icon).
        NSApp.activate()
        let alert = NSAlert()
        alert.messageText = count == 1 ? "Stop 1 service and quit?" : "Stop \(count) services and quit?"
        alert.informativeText = model.activeMachineServiceCount == 0
            ? "Running services are stopped before localhost quits."
            : "Running services are stopped before localhost quits. Those started from here on other machines stop too, unless localhost is open on them."
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
            Button("Connect Agents…") {
                openWindow(id: "main")
                model.showConnectAgents()
            }
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
            Divider()
            Button("Show Welcome…") {
                openWindow(id: "main")
                model.showWelcome()
            }
            Button("Show Get Started Checklist") {
                openWindow(id: "main")
                model.showChecklistAgain()
            }
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
                if let project = model.selectedLocalProject { model.presentedSheet = .addService(projectID: project.id) }
            }
            .keyboardShortcut("n", modifiers: [.command, .shift])
            .disabled(model.selectedLocalProject == nil)
            Button("Detect Services…") {
                if let project = model.selectedLocalProject { model.presentedSheet = .detectServices(projectID: project.id) }
            }
            .disabled(model.selectedLocalProject == nil)
            Divider()
            Button("Import Project…") {
                openWindow(id: "main")
                model.isChoosingImportFile = true
            }
            .disabled(model.core == nil)
            Button("Export Project…") {
                openWindow(id: "main")
                if let project = model.selectedLocalProject { model.exportProject(project.id) }
            }
            .disabled(model.selectedLocalProject == nil)
        }
        CommandGroup(replacing: .textEditing) {
            Button("Find in Log") { model.findInLog() }
                .keyboardShortcut("f")
                .disabled(!model.canFindInLog)
            Button("Clear Log") { model.clearLogs() }
                .keyboardShortcut("l", modifiers: .control)
                .disabled(!model.canClearLogs)
        }
        CommandMenu("Service") {
            let place = model.selectedPlace
            let status = model.selectedService.map { model.status(of: $0, on: place) }
            let isLive = status?.state == .starting || status?.state == .running
            Button(isLive ? "Restart" : "Start") {
                guard let id = model.selectedServiceID else { return }
                isLive ? model.restart(id, on: place) : model.start(id, on: place)
            }
            .keyboardShortcut("r")
            .disabled(status == nil || status?.state == .stopping || !model.isReachable(place))
            // In Ports, Stop applies to the selected Listener.
            let listener = model.sidebarSelection == .ports ? model.selectedListenerID.flatMap(model.listener) : nil
            Button(listener.map { "Stop \($0.name)" + (model.service(of: $0) == nil ? "…" : "") } ?? "Stop") {
                if let listener { model.requestStop(listener) } else { model.selectedServiceID.map { model.stop($0, on: place) } }
            }
            .keyboardShortcut(".")
            .disabled(listener.map { model.reasonNotStoppable($0) != nil } ?? (!isLive || !model.isReachable(place)))
            Divider()
            Button("Open URL") { model.selectedServiceID.map { model.openURL(of: $0, on: place) } }
                .keyboardShortcut("o", modifiers: [.command, .shift])
                .disabled(!isLive || model.selectedService.flatMap { model.urlToOpen($0, on: place) } == nil || !model.isReachable(place))
            Divider()
            // Another Machine's Services are edited there.
            let localService = place == .thisMac ? model.selectedService : nil
            Button("Edit Service…") { localService.map { model.presentedSheet = .editService($0.id) } }
                .disabled(localService == nil)
            Button("Remove Service…") { localService.map { model.pendingRemoval = .service($0.id) } }
                .disabled(localService == nil)
        }
        CommandMenu("Project") {
            let place = model.selectedPlace
            let shown = model.selectedProject
            Button("Start All") { shown.map { model.startAll(inProject: $0.id, on: place) } }
                .keyboardShortcut("r", modifiers: [.command, .shift])
                .disabled((shown?.services.isEmpty ?? true) || !model.isReachable(place))
            Button("Stop All") { shown.map { model.stopAll(inProject: $0.id, on: place) } }
                .keyboardShortcut(".", modifiers: [.command, .shift])
                .disabled(!(shown.map { $0.services.contains { model.status(of: $0, on: place).state.isActive } } ?? false)
                    || !model.isReachable(place))
            Divider()
            let project = model.selectedLocalProject
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
