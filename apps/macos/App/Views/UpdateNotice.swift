import SwiftUI

/// A newer version, at the foot of the sidebar: Update Now downloads it in
/// the background, then Restart to Update installs it. Nothing shows when
/// there is no update.
struct UpdateNotice: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let updates = model.updates
        if updates.state != .none {
            VStack(alignment: .leading, spacing: 6) {
                switch updates.state {
                case .none:
                    EmptyView()
                case .available(let version, _, let problem):
                    Text("localhost \(version) is available")
                        .fontWeight(.semibold)
                    if let problem {
                        Text(problem)
                            .font(.caption)
                            .foregroundStyle(Color(nsColor: Theme.failed))
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    HStack(spacing: 6) {
                        notesButton
                        Spacer(minLength: 0)
                        Button("Later") { updates.putOff() }
                            .buttonStyle(PillButtonStyle(isCompact: true))
                        if problem == nil {
                            Button("Update Now") { updates.download() }
                                .buttonStyle(AccentButtonStyle(isCompact: true))
                        } else {
                            Button("Try Again") { updates.retry() }
                                .buttonStyle(AccentButtonStyle(isCompact: true))
                        }
                    }
                case .downloading(let version, _, let progress):
                    Text("Downloading localhost \(version)…")
                        .fontWeight(.semibold)
                    ProgressView(value: progress ?? 0)
                        .progressViewStyle(.linear)
                        .tint(Theme.accent)
                        .accessibilityLabel("Downloading localhost \(version)")
                case .ready(let version, _):
                    Text("localhost \(version) is ready")
                        .fontWeight(.semibold)
                    Text("Restarting stops running services first.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    HStack(spacing: 6) {
                        notesButton
                        Spacer(minLength: 0)
                        Button("Restart to Update") { updates.install() }
                            .buttonStyle(AccentButtonStyle(isCompact: true))
                    }
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
            .accessibilityLabel("Update")
        }
    }

    private var notesButton: some View {
        Button("Release Notes") { model.updates.openReleaseNotes() }
            .buttonStyle(.plain)
            .font(.caption)
            .foregroundStyle(Theme.accentText)
            .pointerStyle(.link)
    }
}
