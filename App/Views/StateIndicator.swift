import LocalhostCore
import SwiftUI

/// The state dot: ● Running, ◐ Starting/Stopping, ○ Stopped/Exited, ! Failed.
struct StateIndicator: View {
    /// `nil` means "nothing to show a state for" (a Project without Services).
    let state: ServiceState?

    var body: some View {
        Image(systemName: symbol)
            .font(.system(size: 10, weight: .semibold))
            .foregroundStyle(color)
            .frame(width: 14)
            .contentTransition(.symbolEffect(.replace))
            .animation(.easeOut(duration: 0.2), value: state)
            .accessibilityLabel(state.map(Self.label) ?? "No services")
            .help(state.map(Self.label) ?? "No services")
    }

    private var symbol: String {
        switch state {
        case .running: "circle.fill"
        case .starting, .stopping: "circle.lefthalf.filled"
        case .failed: "exclamationmark.circle.fill"
        case .stopped, .exited, nil: "circle"
        }
    }

    private var color: Color {
        switch state {
        case .running: .green
        case .starting, .stopping: .orange
        case .failed: .red
        case .stopped, .exited: .secondary
        case nil: Color.secondary.opacity(0.5)
        }
    }

    static func label(_ state: ServiceState) -> String {
        switch state {
        case .stopped: "Stopped"
        case .starting: "Starting"
        case .running: "Running"
        case .stopping: "Stopping"
        case .exited: "Exited"
        case .failed: "Failed"
        }
    }
}

extension ServiceStatus {
    /// Running although its Port has not accepted connections in time.
    var hasPortWarning: Bool {
        state == .running && run?.isPortUnresponsive == true
    }

    /// e.g. "Running", "Exited · code 0", "Failed · exit 1", "Failed · Command not found."
    var summary: String {
        switch state {
        case .stopped: return "Stopped"
        case .starting:
            if let port = run?.port, run?.endedAt == nil, awaitedDependencies.isEmpty { return "Starting · waiting for port \(port)…" }
            return "Starting…"
        case .running:
            if hasPortWarning, let port = run?.port { return "Running · port \(port) not open" }
            return "Running"
        case .stopping: return "Stopping…"
        case .exited: return "Exited · code 0"
        case .failed:
            if let failure = run?.failure { return "Failed · \(failure.message)" }
            switch run?.termination {
            case .exited(let code): return "Failed · exit \(code)"
            case .signaled(let signal): return "Failed · \(Termination.signalName(signal))"
            case nil: return "Failed"
            }
        }
    }
}
