/// A colour from an ANSI SGR sequence. Clients map indexed colours to their theme.
public enum ANSIColor: Sendable, Hashable {
    /// 0–7 standard, 8–15 bright, 16–255 the xterm 256-colour palette.
    case indexed(UInt8)
    case rgb(red: UInt8, green: UInt8, blue: UInt8)
}

/// The text attributes set by ANSI SGR sequences.
public struct TextStyle: Sendable, Hashable {
    public var foreground: ANSIColor?
    public var background: ANSIColor?
    public var isBold = false
    public var isDim = false
    public var isItalic = false
    public var isUnderlined = false
    public var isInverse = false
    public var isStrikethrough = false

    public init(
        foreground: ANSIColor? = nil,
        background: ANSIColor? = nil,
        isBold: Bool = false,
        isDim: Bool = false,
        isItalic: Bool = false,
        isUnderlined: Bool = false,
        isInverse: Bool = false,
        isStrikethrough: Bool = false
    ) {
        self.foreground = foreground
        self.background = background
        self.isBold = isBold
        self.isDim = isDim
        self.isItalic = isItalic
        self.isUnderlined = isUnderlined
        self.isInverse = isInverse
        self.isStrikethrough = isStrikethrough
    }
}

/// One line of output split into runs of identically styled text.
public struct StyledLine: Sendable, Hashable {
    public struct Span: Sendable, Hashable {
        public var text: String
        public var style: TextStyle

        public init(text: String, style: TextStyle) {
            self.text = text
            self.style = style
        }
    }

    public var spans: [Span]

    public var plainText: String { spans.map(\.text).joined() }
}

/// Lightweight ANSI handling for log lines: SGR colours and attributes become
/// styles; every other escape sequence and control character is removed.
///
/// Each line is parsed on its own, starting unstyled. A carriage return keeps
/// only what follows it, as a terminal would show a redrawn progress line.
public enum ANSI {
    public static func parse(_ line: String) -> StyledLine {
        var parser = Parser()
        parser.parse(Array(line.unicodeScalars))
        return StyledLine(spans: parser.spans)
    }

    public static func strip(_ line: String) -> String {
        parse(line).plainText
    }

    private struct Parser {
        var spans: [StyledLine.Span] = []
        var style = TextStyle()

        mutating func parse(_ scalars: [Unicode.Scalar]) {
            var index = 0
            while index < scalars.count {
                let scalar = scalars[index]
                switch scalar {
                case "\u{1B}":
                    index = skipEscape(scalars, from: index + 1)
                    continue
                case "\r":
                    spans.removeAll()
                case "\t":
                    append(scalar)
                default:
                    if scalar.value >= 0x20, scalar.value != 0x7F { append(scalar) }
                }
                index += 1
            }
        }

        /// Consumes the escape sequence after an ESC and returns the index after it.
        mutating func skipEscape(_ scalars: [Unicode.Scalar], from start: Int) -> Int {
            guard start < scalars.count else { return start }
            switch scalars[start] {
            case "[":
                // CSI: parameter and intermediate bytes, then a final byte in 0x40–0x7E.
                var index = start + 1
                while index < scalars.count, !(0x40...0x7E).contains(scalars[index].value) { index += 1 }
                guard index < scalars.count else { return index }
                if scalars[index] == "m" {
                    applySGR(String(String.UnicodeScalarView(scalars[(start + 1)..<index])))
                }
                return index + 1
            case "]", "P", "X", "^", "_":
                // OSC, DCS, SOS, PM, APC: a string ended by BEL or ESC \.
                var index = start + 1
                while index < scalars.count {
                    if scalars[index] == "\u{07}" { return index + 1 }
                    if scalars[index] == "\u{1B}" { return min(index + 2, scalars.count) }
                    index += 1
                }
                return index
            case "(", ")", "*", "+":
                // Character set designation: one more byte.
                return min(start + 2, scalars.count)
            default:
                return start + 1
            }
        }

        mutating func append(_ scalar: Unicode.Scalar) {
            if spans.last?.style == style {
                spans[spans.count - 1].text.unicodeScalars.append(scalar)
            } else {
                spans.append(.init(text: String(scalar), style: style))
            }
        }

        mutating func applySGR(_ parameters: String) {
            let codes = parameters.split(separator: ";", omittingEmptySubsequences: false).map { Int($0) ?? 0 }
            var index = 0
            while index < codes.count {
                let code = codes[index]
                switch code {
                case 0: style = TextStyle()
                case 1: style.isBold = true
                case 2: style.isDim = true
                case 3: style.isItalic = true
                case 4: style.isUnderlined = true
                case 7: style.isInverse = true
                case 9: style.isStrikethrough = true
                case 22: style.isBold = false; style.isDim = false
                case 23: style.isItalic = false
                case 24: style.isUnderlined = false
                case 27: style.isInverse = false
                case 29: style.isStrikethrough = false
                case 30...37: style.foreground = .indexed(UInt8(code - 30))
                case 39: style.foreground = nil
                case 40...47: style.background = .indexed(UInt8(code - 40))
                case 49: style.background = nil
                case 90...97: style.foreground = .indexed(UInt8(code - 90 + 8))
                case 100...107: style.background = .indexed(UInt8(code - 100 + 8))
                case 38, 48:
                    let (color, consumed) = Self.extendedColor(codes[(index + 1)...])
                    if let color {
                        if code == 38 { style.foreground = color } else { style.background = color }
                    }
                    index += consumed
                default: break
                }
                index += 1
            }
        }

        /// Reads `5;n` or `2;r;g;b`; returns the colour (if valid) and how many codes it used.
        static func extendedColor(_ codes: ArraySlice<Int>) -> (ANSIColor?, Int) {
            let codes = Array(codes)
            func byte(_ index: Int) -> UInt8? {
                index < codes.count ? UInt8(exactly: codes[index]) : nil
            }
            switch codes.first {
            case 5:
                guard let n = byte(1) else { return (nil, codes.count) }
                return (.indexed(n), 2)
            case 2:
                guard let r = byte(1), let g = byte(2), let b = byte(3) else { return (nil, codes.count) }
                return (.rgb(red: r, green: g, blue: b), 4)
            default:
                return (nil, 0)
            }
        }
    }
}
