import AppKit
import SwiftUI

/// The Settings window (⌘, or the sidebar's gear): a rail of sections in
/// the sidebar's glass and the selected section beside it.
struct SettingsView: View {
    enum Section: String, CaseIterable, Identifiable {
        case general, appearance, ports

        var id: Self { self }

        var title: String {
            switch self {
            case .general: "General"
            case .appearance: "Appearance"
            case .ports: "Ports"
            }
        }

        var systemImage: String {
            switch self {
            case .general: "gearshape"
            case .appearance: "circle.lefthalf.filled"
            case .ports: "network"
            }
        }
    }

    @SceneStorage("settings.section") private var section = Section.appearance

    var body: some View {
        HStack(spacing: 0) {
            rail
            Divider()
            ScrollView {
                Group {
                    switch section {
                    case .general: GeneralSettings()
                    case .appearance: AppearanceSettings()
                    case .ports: PortsSettings()
                    }
                }
                .padding(.horizontal, 26)
                .padding(.top, 34)
                .padding(.bottom, 20)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .scrollBounceBehavior(.basedOnSize)
            .columnGlass()
        }
        // Under the hidden title bar too, so the rail reaches the top.
        .ignoresSafeArea()
        .frame(width: 640, height: 440)
    }

    private var rail: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(Section.allCases) { item in
                Button { section = item } label: {
                    Label(item.title, systemImage: item.systemImage)
                        .labelStyle(RailLabelStyle())
                        .fontWeight(section == item ? .semibold : .regular)
                        .foregroundStyle(section == item ? AnyShapeStyle(Theme.accentText) : AnyShapeStyle(.primary))
                        .padding(.horizontal, 9)
                        .frame(maxWidth: .infinity, minHeight: 28, alignment: .leading)
                        .background(section == item ? Theme.accent.opacity(0.14) : .clear, in: .rect(cornerRadius: 7))
                        .contentShape(.rect)
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(section == item ? .isSelected : [])
            }
            Spacer()
        }
        .padding(.horizontal, 10)
        // Clears the traffic lights.
        .padding(.top, 46)
        .frame(width: 178)
        .sidebarGlass()
    }
}

private struct RailLabelStyle: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 8) {
            configuration.icon
                .font(.system(size: 12))
                .frame(width: 16)
            configuration.title
        }
    }
}

// MARK: Sections

private struct SectionHeader: View {
    let title: String
    let subtitle: String

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.system(size: 18, weight: .semibold))
            Text(subtitle)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(.bottom, 16)
    }
}

/// A setting: its name and a line of explanation, the control on the right.
struct SettingsRow<Control: View>: View {
    let title: String
    var detail: String?
    @ViewBuilder let control: Control

    var body: some View {
        HStack(alignment: .center, spacing: 16) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                if let detail {
                    Text(detail)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer(minLength: 0)
            control
        }
        .padding(.vertical, 11)
        .overlay(alignment: .top) { Divider() }
    }
}

private struct GeneralSettings: View {
    @Environment(AppModel.self) private var model
    @AppStorage(AppModel.editorApplicationKey) private var editorPath: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            SectionHeader(title: "General", subtitle: "How localhost works with the rest of your Mac.")
            SettingsRow(title: "Editor", detail: "“Open in Editor” (⌥⌘E) opens projects in this app.") {
                HStack(spacing: 8) {
                    if let editorPath {
                        Image(nsImage: NSWorkspace.shared.icon(forFile: editorPath))
                            .resizable()
                            .frame(width: 18, height: 18)
                        Text(FileManager.default.displayName(atPath: editorPath))
                    } else {
                        Text("None").foregroundStyle(.secondary)
                    }
                    Button("Choose…") { AppModel.chooseEditor() }
                        .buttonStyle(PillButtonStyle())
                }
            }
            @Bindable var updates = model.updates
            SettingsRow(title: "Check for updates automatically",
                        detail: "At launch and every few hours. A new version is offered, never installed without asking.") {
                Toggle("Check for updates automatically", isOn: $updates.isAutomaticCheckEnabled)
                    .labelsHidden()
                    .toggleStyle(.switch)
                    .tint(Theme.accent)
            }
            SettingsRow(title: "Check for updates now") {
                Button("Check Now") { model.updates.checkForUpdates() }
                    .buttonStyle(PillButtonStyle())
            }
        }
    }
}

private struct AppearanceSettings: View {
    @AppStorage(Preferences.appearanceKey) private var appearance = Preferences.Appearance.system
    @AppStorage(Preferences.translucencyKey) private var isTranslucent = true
    @AppStorage(Preferences.logFontSizeKey) private var logFontSize = Preferences.defaultLogFontSize

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            SectionHeader(title: "Appearance", subtitle: "How localhost looks. Service colours and state glyphs adapt to both themes.")
            Text("THEME")
                .font(.system(size: 11, weight: .semibold))
                .tracking(0.5)
                .foregroundStyle(.secondary)
                .padding(.bottom, 9)
            HStack(spacing: 12) {
                ForEach(Preferences.Appearance.allCases) { option in
                    ThemeTile(appearance: option, isSelected: appearance == option) { appearance = option }
                }
            }
            .padding(.bottom, 20)
            SettingsRow(title: "Translucent sidebar and columns",
                        detail: "The desktop shows through, blurred. Off makes every surface solid; Reduce Transparency always does.") {
                Toggle("Translucent sidebar and columns", isOn: $isTranslucent)
                    .labelsHidden()
                    .toggleStyle(.switch)
                    .tint(Theme.accent)
            }
            SettingsRow(title: "Log font size", detail: "The log and the commands in service rows.") {
                Picker("Log font size", selection: $logFontSize) {
                    ForEach(Preferences.logFontSizes, id: \.self) { size in
                        Text("\(Int(size)) pt").tag(size)
                    }
                }
                .labelsHidden()
                .fixedSize()
            }
        }
        .onChange(of: appearance) { Preferences.applyAppearance(appearance.rawValue) }
    }
}

/// A small picture of the window in a theme; System is half light, half dark.
private struct ThemeTile: View {
    let appearance: Preferences.Appearance
    let isSelected: Bool
    let select: () -> Void

    var body: some View {
        Button(action: select) {
            VStack(spacing: 7) {
                preview
                    .frame(height: 78)
                    .clipShape(.rect(cornerRadius: 9))
                    .overlay {
                        RoundedRectangle(cornerRadius: 9)
                            .strokeBorder(isSelected ? Theme.accent : Color.primary.opacity(0.12), lineWidth: isSelected ? 2 : 1)
                    }
                Text(appearance.title)
                    .font(.caption)
                    .fontWeight(isSelected ? .semibold : .regular)
                    .foregroundStyle(isSelected ? AnyShapeStyle(Theme.accentText) : AnyShapeStyle(.primary))
            }
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .frame(maxWidth: .infinity)
        .accessibilityLabel("\(appearance.title) theme")
        .accessibilityAddTraits(isSelected ? .isSelected : [])
    }

    @ViewBuilder
    private var preview: some View {
        switch appearance {
        case .light: window(dark: false)
        case .dark: window(dark: true)
        case .system:
            window(dark: false)
                .overlay {
                    window(dark: true).mask {
                        HStack(spacing: 0) {
                            Color.clear
                            Color.black
                        }
                    }
                }
        }
    }

    private func window(dark: Bool) -> some View {
        HStack(spacing: 0) {
            Color(white: dark ? 0.15 : 0.92).frame(width: 30)
            Color(white: dark ? 0.12 : 0.97).frame(width: 30)
            VStack(alignment: .leading, spacing: 4) {
                Capsule().fill(Color(red: 1, green: 0.36, blue: 0.54)).frame(width: 18, height: 3)
                Capsule().fill(Color(white: dark ? 0.35 : 0.75)).frame(width: 30, height: 3)
                Capsule().fill(Color(white: dark ? 0.35 : 0.75)).frame(width: 24, height: 3)
                Spacer()
            }
            .padding(8)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .background(Color(white: dark ? 0.08 : 0.99))
        }
    }
}

private struct PortsSettings: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model
        VStack(alignment: .leading, spacing: 0) {
            SectionHeader(title: "Ports", subtitle: "What happens when a service’s port is already taken.")
            SettingsRow(title: "Start on a free port when a service’s port is in use",
                        detail: "The service gets the next free port as $PORT for that run; its configured port is kept. Services others depend on, or whose port can’t change, are never moved.") {
                Toggle("Start on a free port", isOn: $model.isAutomaticPortMoveEnabled)
                    .labelsHidden()
                    .toggleStyle(.switch)
                    .tint(Theme.accent)
            }
        }
    }
}
