import AppKit
import LocalhostCore
import SwiftUI

/// The menu bar companion: Projects with an active or Failed Service plus
/// favourites, with Start/Stop/Restart, Start All/Stop All and Open URL.
/// No configuration, environments or logs; those live in the main window.
struct MenuBarContent: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openWindow) private var openWindow
    @State private var isVisible = false
    @State private var contentHeight: CGFloat = 0

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if !model.portWarnings.isEmpty {
                portWarnings
                Divider()
            }
            ScrollView {
                projects
                    .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { contentHeight = $0 }
            }
            .scrollBounceBehavior(.basedOnSize)
            .frame(height: min(contentHeight, 460))
            Divider()
            footer
        }
        .frame(width: 340)
        .background { PanelVisibilityReader(isVisible: $isVisible) }
        .onAppear {
            let openWindow = openWindow
            model.showMainWindow = {
                openWindow(id: "main")
                NSApp.activate()
            }
        }
        // Listeners are only scanned while the panel is open, every ~2s.
        .task(id: isVisible) {
            while isVisible, !Task.isCancelled {
                await model.refreshPortWarnings()
                try? await Task.sleep(for: .seconds(2))
            }
        }
    }

    @ViewBuilder
    private var projects: some View {
        let projects = model.menuBarProjects
        if projects.isEmpty {
            VStack(spacing: 4) {
                Text("No services running")
                    .font(.callout.weight(.medium))
                Text("Favourite a project to keep it here.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 20)
        } else {
            LazyVStack(alignment: .leading, spacing: 0) {
                ForEach(projects) { project in
                    MenuBarProjectSection(project: project)
                    if project.id != projects.last?.id { Divider().padding(.horizontal, 10) }
                }
            }
            .padding(.vertical, 4)
        }
    }

    private var portWarnings: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(model.portWarnings) { warning in
                Button {
                    model.reveal(listener: warning.holder)
                } label: {
                    HStack(alignment: .firstTextBaseline, spacing: 6) {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .foregroundStyle(Color(nsColor: Theme.transitioning))
                        Text(Self.description(of: warning, in: model))
                            .fixedSize(horizontal: false, vertical: true)
                        Spacer(minLength: 0)
                    }
                    .font(.caption)
                    .contentShape(.rect)
                }
                .buttonStyle(MenuBarRowButtonStyle())
                .help("Show in Ports")
            }
        }
        .padding(4)
    }

    /// e.g. "Port 3000 (My SaaS › Web) is held by python3 (PID 123)."
    private static func description(of warning: AppModel.PortWarning, in model: AppModel) -> String {
        let owner = model.service(warning.serviceID).map { "\($0.project.name) › \($0.service.name)" } ?? "a service"
        return "Port \(String(warning.port)) (\(owner)) is held by \(warning.holder.name) (PID \(warning.holder.pid))."
    }

    private var footer: some View {
        VStack(spacing: 0) {
            Button {
                model.showMainWindow?()
            } label: {
                HStack {
                    Text("Open localhost")
                    Spacer()
                }
                .contentShape(.rect)
            }
            .buttonStyle(MenuBarRowButtonStyle())
            Button {
                model.quit(origin: .menuBar)
            } label: {
                HStack {
                    Text("Quit")
                    Spacer()
                    Text("⌘Q").foregroundStyle(.secondary)
                }
                .contentShape(.rect)
            }
            .buttonStyle(MenuBarRowButtonStyle())
            .keyboardShortcut("q")
        }
        .padding(4)
    }
}

/// A Project's header (Start All / Stop All) and its Services.
private struct MenuBarProjectSection: View {
    @Environment(AppModel.self) private var model
    let project: Project

    var body: some View {
        let isAnyActive = project.services.contains { model.status(of: $0).state.isActive }
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 6) {
                Button {
                    model.reveal(projectID: project.id)
                } label: {
                    HStack(spacing: 6) {
                        Text(project.name)
                            .font(.callout.weight(.semibold))
                            .lineLimit(1)
                        if project.isFavorite {
                            Image(systemName: "star.fill")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                        Spacer(minLength: 0)
                    }
                    .contentShape(.rect)
                }
                .buttonStyle(.plain)
                .help("Show in localhost")
                MenuBarIconButton("Start All", systemImage: "play.fill") {
                    model.startAll(inProject: project.id, origin: .menuBar)
                }
                .disabled(project.services.isEmpty)
                MenuBarIconButton("Stop All", systemImage: "stop.fill") {
                    model.stopAll(inProject: project.id, origin: .menuBar)
                }
                .disabled(!isAnyActive)
            }
            .padding(.horizontal, 10)
            .padding(.top, 6)
            .padding(.bottom, 2)
            ForEach(project.services) { service in
                MenuBarServiceRow(project: project, service: service)
            }
        }
        .padding(.bottom, 4)
    }
}

private struct MenuBarServiceRow: View {
    @Environment(AppModel.self) private var model
    let project: Project
    let service: Service

    var body: some View {
        let status = model.status(of: service)
        let isLive = status.state == .starting || status.state == .running
        HStack(spacing: 6) {
            Button {
                model.reveal(projectID: project.id, serviceID: service.id)
            } label: {
                HStack(spacing: 6) {
                    Capsule()
                        .fill(Color(nsColor: model.color(of: service)))
                        .frame(width: 3, height: 14)
                    StateIndicator(state: status.state)
                    Text(service.name)
                        .lineLimit(1)
                    Text(status.summary)
                        .font(.caption)
                        .foregroundStyle(status.state == .failed ? AnyShapeStyle(Color(nsColor: Theme.failed)) : AnyShapeStyle(.secondary))
                        .lineLimit(1)
                        .truncationMode(.tail)
                    Spacer(minLength: 0)
                }
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .help("Show in localhost")
            if isLive, model.urlToOpen(service) != nil {
                MenuBarIconButton("Open URL", systemImage: "safari") { model.openURL(of: service.id) }
            }
            if isLive {
                MenuBarIconButton("Restart", systemImage: "arrow.clockwise") { model.restart(service.id, origin: .menuBar) }
                MenuBarIconButton("Stop", systemImage: "stop.fill") { model.stop(service.id, origin: .menuBar) }
            } else if status.state == .stopping {
                ProgressView()
                    .controlSize(.mini)
                    .frame(width: 22)
            } else {
                MenuBarIconButton("Start", systemImage: "play.fill") { model.start(service.id, origin: .menuBar) }
            }
        }
        .font(.callout)
        .padding(.leading, 6)
        .padding(.trailing, 10)
        .padding(.vertical, 2)
    }
}

/// A small borderless icon button with a tooltip.
private struct MenuBarIconButton: View {
    let title: String
    let systemImage: String
    let action: () -> Void

    init(_ title: String, systemImage: String, action: @escaping () -> Void) {
        self.title = title
        self.systemImage = systemImage
        self.action = action
    }

    var body: some View {
        Button(title, systemImage: systemImage, action: action)
            .labelStyle(.iconOnly)
            .buttonStyle(.borderless)
            .font(.system(size: 11))
            .frame(width: 22)
            .help(title)
    }
}

/// A full-width row that highlights on hover, like a menu item.
private struct MenuBarRowButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        HoverRow(label: configuration.label, isPressed: configuration.isPressed)
    }

    private struct HoverRow<Label: View>: View {
        let label: Label
        let isPressed: Bool
        @State private var isHovered = false

        var body: some View {
            label
                .padding(.horizontal, 6)
                .padding(.vertical, 4)
                .background(isHovered || isPressed ? Color.primary.opacity(0.1) : .clear, in: .rect(cornerRadius: 4))
                .onHover { isHovered = $0 }
        }
    }
}

/// Tracks whether the menu bar panel holding this view is on screen, so work
/// like Listener scans only happens while it is.
private struct PanelVisibilityReader: NSViewRepresentable {
    @Binding var isVisible: Bool

    func makeNSView(context: Context) -> NSView {
        let view = WindowObservingView()
        view.onVisibilityChange = { isVisible = $0 }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {}

    final class WindowObservingView: NSView {
        var onVisibilityChange: ((Bool) -> Void)?
        private var observations: [NSObjectProtocol] = []

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            observations.forEach(NotificationCenter.default.removeObserver)
            observations = []
            guard let window else { return }
            let names: [Notification.Name] = [
                NSWindow.didChangeOcclusionStateNotification, NSWindow.didBecomeKeyNotification,
                NSWindow.didResignKeyNotification, NSWindow.willCloseNotification,
            ]
            observations = names.map { name in
                NotificationCenter.default.addObserver(forName: name, object: window, queue: .main) { [weak self] notification in
                    let isClosing = notification.name == NSWindow.willCloseNotification
                    MainActor.assumeIsolated { self?.report(isClosing: isClosing) }
                }
            }
            report(isClosing: false)
        }

        private func report(isClosing: Bool) {
            guard let window else { return }
            let isVisible = !isClosing && window.isVisible && window.occlusionState.contains(.visible)
            // Deferred: this can run during a SwiftUI update.
            DispatchQueue.main.async { [onVisibilityChange] in onVisibilityChange?(isVisible) }
        }
    }
}

/// The menu bar icon, with a red dot while any Service is Failed. A template
/// image can't carry colour, so the dotted variant is drawn in the menu bar's
/// label colour at draw time.
enum MenuBarIcon {
    static func image(hasFailure: Bool) -> NSImage {
        let configuration = NSImage.SymbolConfiguration(pointSize: 14, weight: .regular)
        guard let symbol = NSImage(systemSymbolName: "server.rack", accessibilityDescription: "localhost")?
            .withSymbolConfiguration(configuration)
        else { return NSImage() }
        guard hasFailure else {
            symbol.isTemplate = true
            return symbol
        }
        let dot: CGFloat = 6
        let image = NSImage(size: symbol.size, flipped: false) { rect in
            symbol.draw(in: rect)
            NSColor.labelColor.set()
            rect.fill(using: .sourceAtop)
            let dotRect = NSRect(x: rect.maxX - dot, y: rect.maxY - dot, width: dot, height: dot)
            NSGraphicsContext.current?.compositingOperation = .clear
            NSBezierPath(ovalIn: dotRect.insetBy(dx: -1.5, dy: -1.5)).fill()
            NSGraphicsContext.current?.compositingOperation = .sourceOver
            NSColor.systemRed.set()
            NSBezierPath(ovalIn: dotRect).fill()
            return true
        }
        image.isTemplate = false
        image.accessibilityDescription = "localhost, a service failed"
        return image
    }
}
