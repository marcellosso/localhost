import AppKit
import SwiftUI

/// The app's colours and surfaces ("Graded glass", #26), each resolved for the
/// light or dark appearance. Green, orange and red are kept for Service State;
/// the accent marks only primary actions.
enum Theme {
    // MARK: Accent

    static let accent = Color(nsColor: dynamic(light: 0xE0336A, dark: 0xFF5C8A))
    /// Text and icons on the accent.
    static let onAccent = Color(nsColor: dynamic(light: 0xFFFFFF, dark: 0x1A0B10))
    /// The accent as text on a light accent tint.
    static let accentText = Color(nsColor: dynamic(light: 0xC2185B, dark: 0xFF7FA3))

    // MARK: Service State

    static let running = dynamic(light: 0x1A7F37, dark: 0x3FB950)
    static let transitioning = dynamic(light: 0xBF8700, dark: 0xF0A030)
    static let failed = dynamic(light: 0xCF222E, dark: 0xFF5A4E)
    static let stopped = dynamic(light: 0x8C959F, dark: 0x7D8590)

    // MARK: Service colours

    /// Assigned to a Project's Services by position, so a Service keeps its
    /// colour everywhere: its row, its log prefix, the menu bar, Ports.
    static let serviceColors: [NSColor] = [
        dynamic(light: 0x0969DA, dark: 0x79C0FF),
        dynamic(light: 0x8250DF, dark: 0xB392F0),
        dynamic(light: 0x9A6700, dark: 0xE5C07B),
        dynamic(light: 0xBC4C00, dark: 0xF78C6C),
        dynamic(light: 0x1B7C83, dark: 0x56D4DD),
        dynamic(light: 0xBF3989, dark: 0xF2A8D0),
        dynamic(light: 0x4C5FD5, dark: 0xA8B4FF),
        dynamic(light: 0x6B7A12, dark: 0xC9D18A),
    ]

    static func serviceColor(at index: Int) -> NSColor {
        serviceColors[index % serviceColors.count]
    }

    // MARK: Surfaces

    /// The log: always opaque, for legible text and exact ANSI colours.
    static let logBackground = dynamic(light: 0xFCFCFD, dark: 0x141417)
    /// Laid over the blurred desktop in the services column and its header.
    static let columnTint = dynamic(light: 0xFCFCFD, dark: 0x1A1A1F, lightAlpha: 0.72, darkAlpha: 0.66)
    /// The services column with translucency off.
    static let columnSolid = dynamic(light: 0xF7F7F9, dark: 0x1C1C21)
    /// The sidebar with translucency off, and the Settings rail's tint.
    static let sidebarSolid = dynamic(light: 0xEDECF0, dark: 0x232329)
    static let sidebarTint = dynamic(light: 0xF6F5F8, dark: 0x202026, lightAlpha: 0.55, darkAlpha: 0.48)

    private static func dynamic(
        light: UInt32, dark: UInt32, lightAlpha: CGFloat = 1, darkAlpha: CGFloat = 1
    ) -> NSColor {
        NSColor(name: nil) { appearance in
            appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
                ? color(dark, alpha: darkAlpha) : color(light, alpha: lightAlpha)
        }
    }

    private static func color(_ hex: UInt32, alpha: CGFloat) -> NSColor {
        NSColor(
            srgbRed: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255,
            blue: CGFloat(hex & 0xFF) / 255, alpha: alpha
        )
    }
}

extension View {
    /// The middle step of the glass gradient: the desktop shows through,
    /// blurred and tinted, less than in the sidebar. Solid with translucency
    /// off in Settings, and under Reduce Transparency.
    func columnGlass() -> some View {
        modifier(Glass(tint: Theme.columnTint, solid: Theme.columnSolid))
    }

    /// The most translucent step, like the sidebar: the Settings rail.
    func sidebarGlass() -> some View {
        modifier(Glass(tint: Theme.sidebarTint, solid: Theme.sidebarSolid))
    }
}

private struct Glass: ViewModifier {
    @AppStorage(Preferences.translucencyKey) private var isTranslucent = true
    let tint: NSColor
    let solid: NSColor

    func body(content: Content) -> some View {
        content.background {
            Group {
                if isTranslucent {
                    ZStack {
                        BehindWindowBlur()
                        Color(nsColor: tint)
                    }
                } else {
                    Color(nsColor: solid)
                }
            }
            .ignoresSafeArea()
        }
    }
}

/// The blurred desktop behind the window. AppKit makes it opaque by itself
/// under Reduce Transparency.
private struct BehindWindowBlur: NSViewRepresentable {
    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.blendingMode = .behindWindow
        view.material = .sidebar
        view.state = .followsWindowActiveState
        return view
    }

    func updateNSView(_ view: NSVisualEffectView, context: Context) {}
}

/// Makes the toolbar one strip across the services column and the log: the
/// divider between them would run up through the toolbar. The split view's
/// dividers are drawn clear and the services/log divider view stays hidden
/// (AppKit shows it again on layout and when the window becomes key). The
/// dividers can still be dragged.
struct ToolbarStripSplitView: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async { Self.adopt(from: view) }
        return view
    }

    func updateNSView(_ view: NSView, context: Context) {
        DispatchQueue.main.async { Self.adopt(from: view) }
    }

    private static func adopt(from view: NSView) {
        guard let root = view.window?.contentView?.superview, let splitView = splitView(in: root) else { return }
        if type(of: splitView) == NSSplitView.self {
            // No stored properties, so the swap keeps the instance's layout.
            object_setClass(splitView, StripSplitView.self)
        }
        splitView.needsLayout = true
        splitView.needsDisplay = true
    }

    private static func splitView(in view: NSView) -> NSSplitView? {
        if let splitView = view as? NSSplitView, splitView.isVertical, splitView.arrangedSubviews.count == 3 {
            return splitView
        }
        return view.subviews.lazy.compactMap(splitView(in:)).first
    }
}

private final class StripSplitView: NSSplitView {
    override var dividerColor: NSColor { .clear }

    override func layout() {
        super.layout()
        hideDivider()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        hideDivider()
    }

    /// The services/log divider; the sidebar's is a different (vibrant) view.
    private func hideDivider() {
        for divider in subviews where type(of: divider) == NSClassFromString("NSSplitDividerView") {
            divider.isHidden = true
        }
    }
}
