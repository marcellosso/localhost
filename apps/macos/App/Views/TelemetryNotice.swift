import LocalhostCore
import SwiftUI

/// Said once, at the foot of the sidebar, the first time a build that sends
/// anonymous usage data opens: what it sends and how to turn it off. Either
/// button dismisses it for good.
struct TelemetryNotice: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openWindow) private var openWindow
    @AppStorage(SettingsView.sectionKey) private var settingsSection = SettingsView.Section.appearance

    var body: some View {
        if model.isTelemetryNoticePending {
            VStack(alignment: .leading, spacing: 6) {
                Text("localhost sends anonymous usage data")
                    .fontWeight(.semibold)
                Text("\(AppModel.telemetrySummary) Turn it off in Settings › General, or set \(Text("DO_NOT_TRACK=1").monospaced()).")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .wrapsInSidebarNotice()
                HStack(spacing: 6) {
                    Link("Privacy", destination: Telemetry.privacyURL)
                        .font(.caption)
                        .foregroundStyle(Theme.accentText)
                        .pointerStyle(.link)
                    Spacer(minLength: 0)
                    Button("Settings…") {
                        model.acknowledgeTelemetryNotice()
                        settingsSection = .general
                        openWindow(id: "settings")
                    }
                    .buttonStyle(PillButtonStyle(isCompact: true))
                    Button("OK") { model.acknowledgeTelemetryNotice() }
                        .buttonStyle(AccentButtonStyle(isCompact: true))
                }
            }
            .font(.callout)
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Theme.accent.opacity(0.08), in: .rect(cornerRadius: 8))
            .overlay { RoundedRectangle(cornerRadius: 8).strokeBorder(Theme.accent.opacity(0.28)) }
            .padding(.horizontal, 10)
            .padding(.bottom, 10)
            .accessibilityElement(children: .contain)
            .accessibilityLabel("Anonymous usage data")
        }
    }
}
