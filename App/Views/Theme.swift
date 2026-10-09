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
    static let columnTint = dynamic(light: 0xFCFCFD, dark: 0x1A1A1F, lightAlpha: 0.86, darkAlpha: 0.80)

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
    /// blurred and tinted, less than in the sidebar. Opaque under Reduce
    /// Transparency.
    func columnGlass() -> some View {
        background {
            ZStack {
                BehindWindowBlur()
                Color(nsColor: Theme.columnTint)
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
