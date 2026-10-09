import LocalhostCore
import SwiftUI

/// The state glyph. Its shape carries the state without colour: a dot with a
/// halo for Running, a pulsing ring for Starting/Stopping (still under Reduce
/// Motion), a diamond for Failed, a hollow ring for Stopped/Exited.
struct StateIndicator: View {
    /// `nil` means "nothing to show a state for" (a Project without Services).
    let state: ServiceState?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        glyph
            .frame(width: 14, height: 14)
            .animation(.easeOut(duration: 0.2), value: state)
            .accessibilityElement()
            .accessibilityLabel(state.map(Self.label) ?? "No services")
            .help(state.map(Self.label) ?? "No services")
    }

    @ViewBuilder
    private var glyph: some View {
        switch state {
        case .running:
            Circle()
                .fill(Color(nsColor: Theme.running))
                .frame(width: 8, height: 8)
                .background { Circle().fill(Color(nsColor: Theme.running).opacity(0.22)).frame(width: 13, height: 13) }
        case .starting, .stopping:
            if reduceMotion {
                ring(Theme.transitioning, width: 2)
            } else {
                PhaseAnimator([false, true]) { isDimmed in
                    ring(Theme.transitioning, width: 2)
                        .opacity(isDimmed ? 0.35 : 1)
                        .scaleEffect(isDimmed ? 0.8 : 1)
                } animation: { _ in .easeInOut(duration: 0.7) }
            }
        case .failed:
            RoundedRectangle(cornerRadius: 1.5)
                .fill(Color(nsColor: Theme.failed))
                .frame(width: 7, height: 7)
                .rotationEffect(.degrees(45))
                .background { Circle().fill(Color(nsColor: Theme.failed).opacity(0.22)).frame(width: 13, height: 13) }
        case .stopped, .exited:
            ring(Theme.stopped, width: 1.5)
        case nil:
            ring(Theme.stopped.withAlphaComponent(0.5), width: 1.5)
        }
    }

    private func ring(_ color: NSColor, width: CGFloat) -> some View {
        Circle()
            .strokeBorder(Color(nsColor: color), lineWidth: width)
            .frame(width: 9, height: 9)
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

    /// Why the active Run is not on the Service's Port, e.g. "Port 3000 is
    /// in use by python3 (PID 123), so this run uses port 3001."
    var portChangeExplanation: String? {
        guard state.isActive, let run, let port = run.port, let change = run.portChange else { return nil }
        switch change.reason {
        case .portInUse(let holder?):
            return "Port \(change.configuredPort) is in use by \(holder.name) (PID \(holder.pid)), so this run uses port \(port)."
        case .portInUse(nil):
            return "Port \(change.configuredPort) is in use, so this run uses port \(port)."
        case .requested:
            return "Started on port \(port) instead of \(change.configuredPort)."
        }
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
            if let port = run?.port, let change = run?.portChange {
                switch change.reason {
                case .portInUse: return "Running · moved to port \(port)"
                case .requested: return "Running · on port \(port)"
                }
            }
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
