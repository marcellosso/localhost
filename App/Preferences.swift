import AppKit

/// App-wide look preferences, set in Settings › Appearance. They are UI
/// state, kept in UserDefaults, not part of the core.
enum Preferences {
    static let appearanceKey = "appearance"
    static let translucencyKey = "translucentSurfaces"
    static let logFontSizeKey = "logFontSize"

    enum Appearance: String, CaseIterable, Identifiable {
        case system, light, dark

        var id: Self { self }

        var title: String {
            switch self {
            case .system: "System"
            case .light: "Light"
            case .dark: "Dark"
            }
        }

        var nsAppearance: NSAppearance? {
            switch self {
            case .system: nil
            case .light: NSAppearance(named: .aqua)
            case .dark: NSAppearance(named: .darkAqua)
            }
        }
    }

    static let logFontSizes: [Double] = [10, 11, 12, 13, 14, 16]
    static let defaultLogFontSize: Double = 12

    static var logFontSize: CGFloat {
        let size = UserDefaults.standard.double(forKey: logFontSizeKey)
        return size > 0 ? size : defaultLogFontSize
    }

    /// Applies the chosen appearance to every window, the menu bar panel included.
    @MainActor
    static func applyAppearance(_ raw: String? = UserDefaults.standard.string(forKey: appearanceKey)) {
        NSApp.appearance = raw.flatMap(Appearance.init(rawValue:))?.nsAppearance
    }
}
