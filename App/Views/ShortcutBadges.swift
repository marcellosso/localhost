import AppKit
import SwiftUI

/// Which rows show their ⌘1–9 / ⌥⌘1–9 number badge.
enum ShortcutBadgeMode {
    case none
    /// ⌘ held: sidebar Project rows.
    case projects
    /// ⌥⌘ held: the selected Project's Service rows.
    case services
}

extension EnvironmentValues {
    @Entry var shortcutBadgeMode: ShortcutBadgeMode = .none
}

extension View {
    /// Shows badges while ⌘ or ⌥⌘ is held ~0.4s in this window.
    func shortcutBadgeHost() -> some View {
        modifier(ShortcutBadgeHost())
    }

    /// Overlays "⌘n" / "⌥⌘n" on the row while `mode` badges show. `number` is
    /// 1-based; rows past 9 have no shortcut and no badge.
    func shortcutBadge(_ number: Int?, shownIn mode: ShortcutBadgeMode) -> some View {
        modifier(ShortcutBadge(number: number, mode: mode))
    }
}

private struct ShortcutBadgeHost: ViewModifier {
    @State private var monitor = ModifierHoldMonitor()

    func body(content: Content) -> some View {
        content
            .environment(\.shortcutBadgeMode, monitor.mode)
            .onAppear { monitor.start() }
            .onDisappear { monitor.stop() }
            .onReceive(NotificationCenter.default.publisher(for: NSApplication.didResignActiveNotification)) { _ in
                monitor.reset()
            }
    }
}

private struct ShortcutBadge: ViewModifier {
    @Environment(\.shortcutBadgeMode) private var shownMode
    let number: Int?
    let mode: ShortcutBadgeMode

    func body(content: Content) -> some View {
        content.overlay(alignment: .trailing) {
            if shownMode == mode, let number, (1...9).contains(number) {
                Text("\(mode == .services ? "⌥⌘" : "⌘")\(number)")
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 5)
                    .padding(.vertical, 1)
                    .background(.regularMaterial, in: .rect(cornerRadius: 4))
                    .overlay(RoundedRectangle(cornerRadius: 4).strokeBorder(.separator))
                    .transition(.opacity)
            }
        }
        .animation(.easeOut(duration: 0.1), value: shownMode == mode)
    }
}

/// Watches the modifier keys: ⌘ alone or ⌥⌘ held for ~0.4s turns badges on;
/// any other combination turns them off at once. Pressing a key before the
/// delay (e.g. ⌘R) cancels it, so quick shortcuts never flash badges.
@Observable
private final class ModifierHoldMonitor {
    private(set) var mode: ShortcutBadgeMode = .none

    @ObservationIgnored private var eventMonitor: Any?
    @ObservationIgnored private var pending: Task<Void, Never>?

    func start() {
        guard eventMonitor == nil else { return }
        eventMonitor = NSEvent.addLocalMonitorForEvents(matching: [.flagsChanged, .keyDown]) { [weak self] event in
            MainActor.assumeIsolated {
                if event.type == .keyDown {
                    self?.pending?.cancel()
                } else {
                    self?.update(event.modifierFlags)
                }
            }
            return event
        }
    }

    func stop() {
        eventMonitor.map(NSEvent.removeMonitor)
        eventMonitor = nil
        reset()
    }

    func reset() {
        pending?.cancel()
        mode = .none
    }

    private func update(_ flags: NSEvent.ModifierFlags) {
        let held = flags.intersection([.command, .option, .shift, .control])
        let target: ShortcutBadgeMode = switch held {
        case [.command]: .projects
        case [.command, .option]: .services
        default: .none
        }
        pending?.cancel()
        // Already showing (e.g. ⌘ → ⌥⌘): switch at once.
        guard target != .none, mode == .none else {
            mode = target
            return
        }
        pending = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(400))
            guard !Task.isCancelled else { return }
            self?.mode = target
        }
    }
}
