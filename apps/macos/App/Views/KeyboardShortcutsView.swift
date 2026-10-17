import SwiftUI

/// Every keyboard shortcut, grouped (Help › Keyboard Shortcuts, ⌘/, or the
/// palette). Every action is also a menu item; this is the list in one place.
struct KeyboardShortcutsView: View {
    struct Shortcut: Identifiable {
        let title: String
        /// Each drawn as a key cap, e.g. ["⌘[", "⌘]"].
        let keys: [String]
        var id: String { title }
    }

    struct Group: Identifiable {
        let title: String
        let shortcuts: [Shortcut]
        var id: String { title }
    }

    static let columns: [[Group]] = [
        [
            Group(title: "Go", shortcuts: [
                Shortcut(title: "Command Palette", keys: ["⌘K"]),
                Shortcut(title: "Project 1–9", keys: ["⌘1–9"]),
                Shortcut(title: "Service 1–9 of the project", keys: ["⌥⌘1–9"]),
                Shortcut(title: "Previous / Next Service", keys: ["⌘[", "⌘]"]),
                Shortcut(title: "Ports", keys: ["⌘0"]),
            ]),
            Group(title: "Service", shortcuts: [
                Shortcut(title: "Start or Restart", keys: ["⌘R"]),
                Shortcut(title: "Stop", keys: ["⌘."]),
                Shortcut(title: "Open URL", keys: ["⇧⌘O"]),
                Shortcut(title: "Open in Terminal", keys: ["⌥⌘T"]),
                Shortcut(title: "New Service", keys: ["⇧⌘N"]),
            ]),
            Group(title: "Project", shortcuts: [
                Shortcut(title: "Start All", keys: ["⇧⌘R"]),
                Shortcut(title: "Stop All", keys: ["⇧⌘."]),
                Shortcut(title: "Open in Editor", keys: ["⌥⌘E"]),
                Shortcut(title: "Reveal in Finder", keys: ["⌥⌘F"]),
                Shortcut(title: "New Project", keys: ["⌘N"]),
            ]),
        ],
        [
            Group(title: "Log", shortcuts: [
                Shortcut(title: "Filter", keys: ["⌘F"]),
                Shortcut(title: "Clear", keys: ["⌃L"]),
                Shortcut(title: "Scroll a line", keys: ["↑", "↓"]),
                Shortcut(title: "Scroll a page", keys: ["space", "⇧space"]),
                Shortcut(title: "Top / Bottom", keys: ["⌘↑", "⌘↓"]),
            ]),
            Group(title: "Columns", shortcuts: [
                Shortcut(title: "Next / Previous column", keys: ["→", "←"]),
                Shortcut(title: "Move in a list", keys: ["↑", "↓"]),
                Shortcut(title: "From the log to the services", keys: ["esc"]),
            ]),
            Group(title: "App", shortcuts: [
                Shortcut(title: "Settings", keys: ["⌘,"]),
                Shortcut(title: "Keyboard Shortcuts", keys: ["⌘/"]),
                Shortcut(title: "Close Window", keys: ["⌘W"]),
                Shortcut(title: "Quit", keys: ["⌘Q"]),
            ]),
        ],
    ]

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Image(systemName: "keyboard")
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(Theme.accent)
                    .frame(width: 34, height: 34)
                    .background(Theme.accent.opacity(0.15), in: .rect(cornerRadius: 9))
                Text("Keyboard Shortcuts")
                    .font(.system(size: 16, weight: .semibold))
                    .accessibilityAddTraits(.isHeader)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 24)
            .padding(.top, 20)
            .padding(.bottom, 14)

            HStack(alignment: .top, spacing: 28) {
                ForEach(Array(Self.columns.enumerated()), id: \.offset) { _, groups in
                    VStack(alignment: .leading, spacing: 18) {
                        ForEach(groups) { group in
                            section(group)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .padding(.horizontal, 24)
            .padding(.bottom, 18)

            HStack(spacing: 8) {
                HStack(spacing: 4) {
                    Text("Hold")
                    KeyHint("⌘")
                    Text("or")
                    KeyHint("⌥⌘")
                    Text("to see each row’s number.")
                }
                .font(.caption)
                .foregroundStyle(.secondary)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("Hold Command, or Option Command, to see each row’s number.")
                Spacer(minLength: 8)
                Button("Done") { dismiss() }
                    .buttonStyle(AccentButtonStyle())
                    .keyboardShortcut(.defaultAction)
            }
            .padding(.horizontal, 24)
            .padding(.vertical, 14)
            .overlay(alignment: .top) { Divider() }
        }
        .frame(width: 640)
        .onExitCommand { dismiss() }
    }

    private func section(_ group: Group) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(group.title)
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.secondary)
                .textCase(.uppercase)
                .accessibilityAddTraits(.isHeader)
            ForEach(group.shortcuts) { shortcut in
                HStack(spacing: 4) {
                    Text(shortcut.title)
                        .font(.callout)
                    Spacer(minLength: 8)
                    ForEach(shortcut.keys, id: \.self) { KeyHint($0) }
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(shortcut.title)
                .accessibilityValue(shortcut.keys.joined(separator: ", "))
            }
        }
    }
}

/// A line that names a shortcut in a key cap, e.g. "or press ⌘R"; for empty
/// states, so they teach the keyboard as well as the button.
struct KeyTip: View {
    let before: String
    let keys: String
    var after: String = ""

    var body: some View {
        HStack(spacing: 4) {
            Text(before)
            KeyHint(keys)
            if !after.isEmpty { Text(after) }
        }
        .font(.callout)
        .foregroundStyle(.secondary)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(before) \(keys) \(after)")
    }
}
