import AppKit
import LocalhostCore
import SwiftUI

extension Animation {
    /// `animation`, or none under Reduce Motion: for movement (rotating
    /// chevrons, rows growing). Fades and colour changes stay animated.
    static func motion(_ animation: Animation) -> Animation? {
        NSWorkspace.shared.accessibilityDisplayShouldReduceMotion ? nil : animation
    }
}

extension String {
    /// The text as VoiceOver should read it: "Running · port 3000" → "Running, port 3000".
    var spoken: String {
        replacingOccurrences(of: " · ", with: ", ").replacingOccurrences(of: "…", with: "")
    }
}

extension AppModel {
    /// A Project's Services for VoiceOver, e.g. "2 of 3 running, 1 failed".
    func accessibilityState(of project: Project) -> String {
        guard !project.services.isEmpty else { return "No services" }
        let states = project.services.map { status(of: $0).state }
        let failed = states.filter { $0 == .failed }.count
        return "\(states.filter(\.isActive).count) of \(states.count) running" + (failed > 0 ? ", \(failed) failed" : "")
    }
}

extension ServiceStatus {
    /// What VoiceOver says about a Service besides its name, e.g. "Running, moved to port 3001".
    var spokenSummary: String {
        let state = StateIndicator.label(state)
        let summary = summary.spoken
        return summary.hasPrefix(state) ? summary : "\(state), \(summary)"
    }
}
