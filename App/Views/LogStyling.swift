import AppKit
import LocalhostCore

extension NSAttributedString.Key {
    /// Marks the per-line stream gutter, which is not part of the output.
    static let logGutter = NSAttributedString.Key("localhost.logGutter")
}

/// Turns output lines into attributed text: a gutter marking the stream, then
/// the line with its ANSI colours and attributes.
enum LogStyling {
    static let font = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
    private static let boldFont = NSFont.monospacedSystemFont(ofSize: 12, weight: .bold)
    private static let italicFont = NSFontManager.shared.convert(font, toHaveTrait: .italicFontMask)
    private static let boldItalicFont = NSFontManager.shared.convert(boldFont, toHaveTrait: .italicFontMask)

    static func attributedLine(_ line: OutputLine) -> NSAttributedString {
        let result = NSMutableAttributedString()
        result.append(gutter(for: line.channel))
        if line.channel == .localhost {
            result.append(NSAttributedString(string: line.text, attributes: [
                .font: italicFont,
                .foregroundColor: NSColor.secondaryLabelColor,
            ]))
        } else {
            for span in ANSI.parse(line.text).spans {
                result.append(NSAttributedString(string: span.text, attributes: attributes(for: span.style)))
            }
        }
        result.append(NSAttributedString(string: "\n", attributes: [.font: font]))
        return result
    }

    private static func gutter(for channel: OutputLine.Channel) -> NSAttributedString {
        let (marker, color): (String, NSColor) = switch channel {
        case .stdout: ("  ", .clear)
        case .stderr: ("▍ ", NSColor.systemRed.withAlphaComponent(0.7))
        case .localhost: ("› ", .tertiaryLabelColor)
        }
        return NSAttributedString(string: marker, attributes: [
            .font: font, .foregroundColor: color, .logGutter: true,
        ])
    }

    private static func attributes(for style: TextStyle) -> [NSAttributedString.Key: Any] {
        var foreground = style.foreground.map(color) ?? .textColor
        var background = style.background.map(color)
        if style.isInverse {
            (foreground, background) = (background ?? .textBackgroundColor, foreground)
        }
        if style.isDim {
            foreground = foreground.withAlphaComponent(0.6)
        }
        let font = switch (style.isBold, style.isItalic) {
        case (true, true): boldItalicFont
        case (true, false): boldFont
        case (false, true): italicFont
        case (false, false): Self.font
        }
        var attributes: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: foreground]
        if let background { attributes[.backgroundColor] = background }
        if style.isUnderlined { attributes[.underlineStyle] = NSUnderlineStyle.single.rawValue }
        if style.isStrikethrough { attributes[.strikethroughStyle] = NSUnderlineStyle.single.rawValue }
        return attributes
    }

    /// Standard and bright colours use adaptive system colours so they stay
    /// readable in light and dark mode; the 256-colour cube and truecolor are exact.
    private static func color(_ color: ANSIColor) -> NSColor {
        switch color {
        case .indexed(let index) where index < 16:
            let palette: [NSColor] = [
                .secondaryLabelColor, .systemRed, .systemGreen, .systemYellow,
                .systemBlue, .systemPurple, .systemCyan, .labelColor,
            ]
            return palette[Int(index % 8)]
        case .indexed(let index) where index < 232:
            let levels: [CGFloat] = [0, 95, 135, 175, 215, 255]
            let value = Int(index) - 16
            return NSColor(
                srgbRed: levels[value / 36] / 255, green: levels[(value / 6) % 6] / 255,
                blue: levels[value % 6] / 255, alpha: 1
            )
        case .indexed(let index):
            let gray = CGFloat(8 + (Int(index) - 232) * 10) / 255
            return NSColor(srgbRed: gray, green: gray, blue: gray, alpha: 1)
        case .rgb(let red, let green, let blue):
            return NSColor(
                srgbRed: CGFloat(red) / 255, green: CGFloat(green) / 255, blue: CGFloat(blue) / 255, alpha: 1
            )
        }
    }
}
