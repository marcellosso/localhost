import AppKit
import SwiftUI

/// A column of the main window, left to right.
enum MainColumn: Int {
    /// Ports and the Projects.
    case sidebar
    /// The selected Project's Services, or the Listeners.
    case list
    /// The log, or the selected Listener.
    case detail
}

/// Asks the main window to focus a column; each request is new, even for the same column.
struct ColumnFocusRequest: Equatable {
    let column: MainColumn
    let id = UUID()
}

/// Moves keyboard focus across the main window's columns, sidebar → Services
/// → log: → (and ↩ in the sidebar) moves right, ← (and esc in the log)
/// moves left. Focus lands on the column's list, or on the log. Keys typed
/// into text fields, and keys while a sheet or alert is up, are left alone.
///
/// Also carries out `AppModel.focusColumn(_:)`, and gives focus back to
/// where it was when the command palette closes.
struct ColumnNavigation: NSViewRepresentable {
    let focusRequest: ColumnFocusRequest?
    let isShowingPalette: Bool

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context: Context) -> NSView {
        context.coordinator.install()
        return context.coordinator.view
    }

    func updateNSView(_ view: NSView, context: Context) {
        let coordinator = context.coordinator
        if focusRequest != coordinator.focusRequest {
            coordinator.focusRequest = focusRequest
            // After SwiftUI has shown what the request is about, e.g. a newly selected Project.
            if let column = focusRequest?.column {
                DispatchQueue.main.async { coordinator.focus(column) }
            }
        }
        if isShowingPalette != coordinator.isShowingPalette {
            coordinator.isShowingPalette = isShowingPalette
            if isShowingPalette {
                coordinator.focusBeforePalette = view.window?.firstResponder as? NSView
            } else {
                DispatchQueue.main.async { coordinator.restoreFocusAfterPalette() }
            }
        }
    }

    static func dismantleNSView(_ view: NSView, coordinator: Coordinator) {
        coordinator.uninstall()
    }

    @MainActor
    final class Coordinator {
        let view = NSView()
        var focusRequest: ColumnFocusRequest?
        var isShowingPalette = false
        weak var focusBeforePalette: NSView?
        private var monitor: Any?

        func install() {
            monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
                let isHandled = MainActor.assumeIsolated { self?.handle(event) ?? false }
                return isHandled ? nil : event
            }
        }

        func uninstall() {
            if let monitor { NSEvent.removeMonitor(monitor) }
            monitor = nil
        }

        /// Focuses the column's log or list; false when it has neither.
        @discardableResult
        func focus(_ column: MainColumn) -> Bool {
            guard let window = view.window, let splitView = window.mainSplitView,
                  column.rawValue < splitView.arrangedSubviews.count,
                  let target = Self.focusTarget(in: splitView.arrangedSubviews[column.rawValue])
            else { return false }
            return window.makeFirstResponder(target)
        }

        func restoreFocusAfterPalette() {
            if let previous = focusBeforePalette, previous.window === view.window, !previous.isHiddenOrHasHiddenAncestor,
               view.window?.makeFirstResponder(previous) == true {
                return
            }
            // What it was on went away, e.g. a jump to another Project.
            if !focus(.list) { focus(.sidebar) }
        }

        private func handle(_ event: NSEvent) -> Bool {
            guard let window = view.window, event.window === window, window.attachedSheet == nil, !isShowingPalette,
                  let splitView = window.mainSplitView,
                  let responder = window.firstResponder as? NSView
            else { return false }
            let modifiers = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
                .subtracting([.function, .numericPad, .capsLock])
            guard modifiers.isEmpty else { return false }
            // A text field's editor, e.g. a search or filter field.
            if let text = responder as? NSTextView, text.isEditable { return false }
            guard let index = splitView.arrangedSubviews.firstIndex(where: responder.isDescendant(of:)),
                  let column = MainColumn(rawValue: index)
            else { return false }

            let escape: UInt16 = 53
            switch (event.specialKey, column) {
            case (.rightArrow, .sidebar), (.carriageReturn, .sidebar), (.enter, .sidebar):
                return focus(.list)
            case (.rightArrow, .list):
                return focus(.detail)
            case (.leftArrow, .list):
                return focus(.sidebar)
            case (.leftArrow, .detail):
                return focus(.list)
            case (_, .detail) where event.keyCode == escape:
                return focus(.list)
            default:
                return false
            }
        }

        /// The log, else the first list, that is showing in `column`.
        private static func focusTarget(in column: NSView) -> NSView? {
            func first<View: NSView>(_ type: View.Type, in view: NSView) -> View? {
                if let match = view as? View, !match.isHiddenOrHasHiddenAncestor { return match }
                return view.subviews.lazy.compactMap { first(type, in: $0) }.first
            }
            return first(LogTextView.self, in: column) ?? first(NSTableView.self, in: column)
        }
    }
}
