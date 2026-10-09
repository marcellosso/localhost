import LocalhostCore
import SwiftUI

/// The primary action: filled with the accent. Used once per surface.
struct AccentButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled
    var isCompact = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .labelStyle(.titleAndIcon)
            .font(.system(size: isCompact ? 11 : 12, weight: .semibold))
            .foregroundStyle(Theme.onAccent)
            .padding(.horizontal, isCompact ? 9 : 11)
            .frame(height: isCompact ? 22 : 26)
            .background(Theme.accent.opacity(configuration.isPressed ? 0.8 : 1), in: .rect(cornerRadius: 7))
            .opacity(isEnabled ? 1 : 0.45)
            .contentShape(.rect)
    }
}

/// A labelled secondary action on a quiet fill. `isProminent` tints it with
/// the accent, for the main action among several.
/// On a selected row of a focused list, both read as white.
struct PillButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled
    @Environment(\.backgroundProminence) private var prominence
    var isProminent = false
    var isCompact = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .labelStyle(.titleAndIcon)
            .font(.system(size: isCompact ? 11 : 12, weight: isProminent ? .semibold : .medium))
            .foregroundStyle(foreground)
            .padding(.horizontal, isCompact ? 9 : 11)
            .frame(height: isCompact ? 22 : 26)
            .background(fill.opacity(configuration.isPressed ? 1.6 : 1), in: .rect(cornerRadius: isCompact ? 6 : 7))
            .opacity(isEnabled ? 1 : 0.35)
            .contentShape(.rect)
    }

    private var foreground: AnyShapeStyle {
        if prominence == .increased { return AnyShapeStyle(isProminent ? Color.black.opacity(0.85) : .white) }
        return isProminent ? AnyShapeStyle(Theme.accentText) : AnyShapeStyle(.primary)
    }

    private var fill: Color {
        if prominence == .increased { return isProminent ? .white.opacity(0.92) : .white.opacity(0.18) }
        return isProminent ? Theme.accent.opacity(0.14) : Color.primary.opacity(0.07)
    }
}

/// A shortcut shown next to the control it triggers, e.g. "⇧⌘N".
struct KeyHint: View {
    let keys: String

    init(_ keys: String) {
        self.keys = keys
    }

    var body: some View {
        Text(keys)
            .font(.system(size: 10, weight: .medium))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 4)
            .padding(.vertical, 1.5)
            .background(Color.primary.opacity(0.07), in: .rect(cornerRadius: 4))
            .accessibilityHidden(true)
    }
}

/// A Service's Port in its colour, e.g. ":3000"; white on a selected row
/// of a focused list.
struct PortPill: View {
    @Environment(\.backgroundProminence) private var prominence
    let port: Int
    let color: NSColor

    var body: some View {
        let color = prominence == .increased ? Color.white : Color(nsColor: color)
        Text(":\(String(port))")
            .font(.system(size: 10.5, weight: .semibold, design: .monospaced))
            .foregroundStyle(color)
            .padding(.horizontal, 5)
            .padding(.vertical, 2)
            .background(color.opacity(prominence == .increased ? 0.2 : 0.12), in: .rect(cornerRadius: 4))
    }
}

/// A Project's Services as a cluster of their colours; inactive ones faded.
struct ServiceDots: View {
    let services: [(color: NSColor, isActive: Bool)]

    var body: some View {
        let rows = stride(from: 0, to: min(services.count, 4), by: 2).map { Array(services[$0..<min($0 + 2, services.count, 4)]) }
        VStack(spacing: 2) {
            ForEach(rows.indices, id: \.self) { row in
                HStack(spacing: 2) {
                    ForEach(rows[row].indices, id: \.self) { index in
                        Circle()
                            .fill(Color(nsColor: rows[row][index].color))
                            .opacity(rows[row][index].isActive ? 1 : 0.35)
                            .frame(width: 5, height: 5)
                    }
                }
                .frame(width: 12, alignment: .leading)
            }
        }
        .frame(width: 14, height: 14)
        .accessibilityHidden(true)
    }
}

/// A borderless `⋯` menu.
struct MoreMenu<Content: View>: View {
    let help: String
    @ViewBuilder let content: Content

    var body: some View {
        Menu {
            content
        } label: {
            Label("More", systemImage: "ellipsis")
                .labelStyle(.iconOnly)
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .help(help)
    }
}

extension AppModel {
    /// Each Service's colour in its Project with whether it is active, for `ServiceDots`.
    func serviceDots(of project: Project) -> [(color: NSColor, isActive: Bool)] {
        project.services.enumerated().map { index, service in
            (Theme.serviceColor(at: index), status(of: service).state.isActive)
        }
    }

    /// The Service whose Run holds the Listener, if any.
    func service(of listener: Listener) -> (project: Project, service: Service)? {
        guard case .run(let serviceID, _) = listener.owner else { return nil }
        return service(serviceID)
    }

    /// The Service's colour: stable for its position in its Project.
    func color(of service: Service) -> NSColor {
        guard let (project, _) = self.service(service.id),
              let index = project.services.firstIndex(where: { $0.id == service.id }) else { return Theme.serviceColor(at: 0) }
        return Theme.serviceColor(at: index)
    }
}

/// A form row whose value is monospaced (a Key, command, port or URL) while
/// its label stays in the system font.
struct MonospacedField: View {
    let label: String
    @Binding var text: String
    let prompt: String

    init(_ label: String, text: Binding<String>, prompt: String) {
        self.label = label
        self._text = text
        self.prompt = prompt
    }

    var body: some View {
        LabeledContent(label) {
            TextField(label, text: $text, prompt: Text(prompt))
                .labelsHidden()
                .font(.body.monospaced())
        }
    }
}

extension View {
    /// A sheet's title above its content: sheets don't show the window title.
    func sheetTitle(_ title: String) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(title)
                .font(.title3.weight(.semibold))
                .padding(.horizontal, 20)
                .padding(.top, 18)
                .accessibilityAddTraits(.isHeader)
            self
        }
    }
}
