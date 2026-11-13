import LocalhostCore
import SwiftUI

/// The first launch's welcome: what a Project, a Service and a Port are, one
/// step each, ending in Add Project…. Skipping, Escape or finishing all mean
/// it never opens by itself again; Help › Show Welcome… opens it anyway.
struct WelcomeView: View {
    @Environment(AppModel.self) private var model
    @State private var index = 0

    private var isLast: Bool { index == Onboarding.welcomeSteps.count - 1 }

    var body: some View {
        let step = Onboarding.welcomeSteps[index]
        VStack(spacing: 0) {
            WelcomePicture(index: index)
                .frame(maxWidth: .infinity)
                .frame(height: 176)
                .background {
                    ZStack {
                        Color(nsColor: Theme.columnSolid)
                        RadialGradient(
                            colors: [Theme.accent.opacity(0.14), .clear],
                            center: UnitPoint(x: 0.5, y: 1.2), startRadius: 0, endRadius: 300
                        )
                    }
                }
                .overlay(alignment: .bottom) { Divider() }
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 6) {
                Text(Onboarding.welcomeTitle.uppercased())
                    .font(.system(size: 11, weight: .semibold))
                    .tracking(0.5)
                    .foregroundStyle(Theme.accentText)
                    .accessibilityLabel("\(Onboarding.welcomeTitle), step \(index + 1) of \(Onboarding.welcomeSteps.count)")
                Text(step.title)
                    .font(.system(size: 17, weight: .bold))
                    .accessibilityAddTraits(.isHeader)
                Text(LocalizedStringKey(step.body))
                    .font(.system(size: 12.5))
                    .lineSpacing(2)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(minHeight: 36, alignment: .top)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 24)
            .padding(.top, 18)
            .padding(.bottom, 4)

            HStack(spacing: 8) {
                Button(isLast ? "Not Now" : "Skip") { model.finishWelcome(addingProject: false) }
                    .buttonStyle(.plain)
                    .foregroundStyle(.secondary)
                    .pointerStyle(.link)
                Spacer(minLength: 8)
                if index > 0 {
                    Button("Back") { go(to: index - 1) }
                        .buttonStyle(PillButtonStyle())
                }
                Button(isLast ? "Add Project…" : "Continue") {
                    if isLast { model.finishWelcome(addingProject: true) } else { go(to: index + 1) }
                }
                .buttonStyle(AccentButtonStyle())
                .keyboardShortcut(.defaultAction)
            }
            .overlay { StepDots(index: index) }
            .padding(.horizontal, 24)
            .padding(.top, 14)
            .padding(.bottom, 18)
        }
        .frame(width: 460)
        .onExitCommand { model.finishWelcome(addingProject: false) }
    }

    /// The next step, said to VoiceOver, since only its text changes.
    private func go(to next: Int) {
        index = next
        let step = Onboarding.welcomeSteps[next]
        AccessibilityNotification.Announcement("Step \(next + 1) of \(Onboarding.welcomeSteps.count), \(step.title)").post()
    }
}

/// Where the user is among the steps; the caption says it to VoiceOver.
private struct StepDots: View {
    let index: Int

    var body: some View {
        HStack(spacing: 6) {
            ForEach(Onboarding.welcomeSteps.indices, id: \.self) { dot in
                Circle()
                    .fill(dot == index ? Theme.accent : Color.primary.opacity(0.11))
                    .frame(width: 6, height: 6)
            }
        }
        .accessibilityHidden(true)
    }
}

/// A small mock of the window for each step: the folder, its Services, then a Port that answers.
private struct WelcomePicture: View {
    let index: Int

    private struct Row {
        let name: String
        let command: String
        let port: Int
    }

    private let rows = [
        Row(name: "Web", command: "pnpm dev", port: 3000),
        Row(name: "Database", command: "docker compose up", port: 5432),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            if index == 0 {
                folder
            } else {
                ForEach(rows.indices, id: \.self) { position in
                    service(rows[position], color: Theme.serviceColor(at: position), isUp: index == 2)
                        .padding(.top, position == 0 ? 0 : 2)
                }
            }
        }
        .font(.system(size: 12.5))
        .padding(8)
        .frame(width: 300)
        .background(Color(nsColor: Theme.raised), in: .rect(cornerRadius: 10))
        .overlay { RoundedRectangle(cornerRadius: 10).strokeBorder(Color.primary.opacity(0.1), lineWidth: 0.5) }
        .shadow(color: .black.opacity(0.1), radius: 10, y: 6)
    }

    private var folder: some View {
        VStack(spacing: 2) {
            HStack(spacing: 8) {
                Image(systemName: "folder")
                Text("~/code/my-saas")
                    .font(.system(size: 12, design: .monospaced))
                Spacer(minLength: 0)
            }
            .foregroundStyle(Theme.accentText)
            .padding(.horizontal, 8)
            .frame(height: 30)
            .overlay {
                RoundedRectangle(cornerRadius: 7)
                    .strokeBorder(Theme.accent.opacity(0.45), style: StrokeStyle(lineWidth: 1, dash: [3, 3]))
            }
            Rectangle()
                .fill(Color.primary.opacity(0.1))
                .frame(width: 1, height: 12)
                .padding(.vertical, 2)
            projectRow("My SaaS", isSelected: true)
            projectRow("Blog", isSelected: false)
        }
    }

    private func projectRow(_ name: String, isSelected: Bool) -> some View {
        HStack(spacing: 8) {
            StateIndicator(state: nil)
            Text(name).fontWeight(.semibold)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 8)
        .frame(height: 28)
        .background(isSelected ? Color.primary.opacity(0.045) : .clear, in: .rect(cornerRadius: 6))
    }

    private func service(_ row: Row, color: NSColor, isUp: Bool) -> some View {
        HStack(spacing: 8) {
            StateIndicator(state: isUp ? .running : .stopped)
            VStack(alignment: .leading, spacing: 0) {
                Text(row.name).fontWeight(.semibold)
                Text(row.command)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .fixedSize()
            }
            Spacer(minLength: 0)
            if isUp {
                PortPill(port: row.port, color: color)
                Text("Running")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(Color(nsColor: Theme.running))
            }
        }
        .padding(.horizontal, 8)
        .frame(minHeight: 40)
        .overlay(alignment: .leading) {
            UnevenRoundedRectangle(topLeadingRadius: 4, bottomLeadingRadius: 4)
                .fill(Color(nsColor: color))
                .frame(width: 3)
        }
    }
}
