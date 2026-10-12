import Foundation

/// A YAML value read strictly from `YAMLLines`, with the line it starts on,
/// for Project files. The subset: block mappings, block sequences of
/// scalars, flow sequences of scalars (`[a, b]`), empty flow mappings (`{}`),
/// plain, single- and double-quoted scalars, comments. Anything else (block
/// scalars, anchors, tags, nested flow collections, …) is rejected with its
/// line rather than skipped.
struct YAMLValue: Equatable {
    struct Entry: Equatable {
        var key: String
        var line: Int
        var value: YAMLValue
    }

    enum Kind: Equatable {
        /// `key:` with nothing after or under it.
        case null
        case scalar(String)
        case sequence([YAMLValue])
        /// In file order.
        case mapping([Entry])
    }

    var line: Int
    var kind: Kind

    /// The document's top-level value; an empty document is `null`.
    static func parse(_ text: String) throws(LocalhostError) -> YAMLValue {
        var parser = Parser(lines: YAMLLines(text).lines)
        return try parser.document()
    }

    private struct Parser {
        let lines: [YAMLLines.Line]
        var index = 0

        mutating func document() throws(LocalhostError) -> YAMLValue {
            guard let first = lines.first else { return YAMLValue(line: 1, kind: .null) }
            let value = try block(indent: first.indent)
            if index < lines.count { throw Self.error(.unexpectedIndentation, lines[index]) }
            return value
        }

        /// The mapping or sequence whose lines are at `indent`.
        mutating func block(indent: Int) throws(LocalhostError) -> YAMLValue {
            let first = lines[index]
            if first.isListItem { return try sequence(indent: indent) }
            var entries: [Entry] = []
            while index < lines.count, lines[index].indent == indent {
                let line = lines[index]
                if line.isListItem { throw Self.error(.expectedKeyValue, line) }
                let (key, rest) = try Self.keyValue(line)
                if entries.contains(where: { $0.key == key }) { throw Self.error(.duplicate, line, field: [key]) }
                index += 1
                entries.append(Entry(key: key, line: line.number, value: try value(after: rest, of: line)))
            }
            return YAMLValue(line: first.number, kind: .mapping(entries))
        }

        mutating func sequence(indent: Int) throws(LocalhostError) -> YAMLValue {
            let first = lines[index]
            var items: [YAMLValue] = []
            while index < lines.count, lines[index].indent == indent, lines[index].isListItem {
                let line = lines[index]
                let rest = String(line.text.dropFirst(1)).trimmingCharacters(in: .whitespaces)
                if (try? Self.keyValue(Line(text: rest))) != nil {
                    throw Self.error(.unsupportedYAML("lists of mappings"), line)
                }
                index += 1
                items.append(try value(after: rest, of: line))
            }
            return YAMLValue(line: first.number, kind: .sequence(items))
        }

        /// The value of `line`, written after its key or dash (`rest`), or on
        /// the lines nested under it.
        mutating func value(after rest: String, of line: YAMLLines.Line) throws(LocalhostError) -> YAMLValue {
            let next = index < lines.count ? lines[index] : nil
            if rest.isEmpty {
                guard let next else { return YAMLValue(line: line.number, kind: .null) }
                // A sequence may sit at its key's indentation.
                if next.indent > line.indent || (next.indent == line.indent && next.isListItem && !line.isListItem) {
                    return try block(indent: next.indent)
                }
                return YAMLValue(line: line.number, kind: .null)
            }
            let value = try Self.inline(rest, line: line.number)
            if let next, next.indent > line.indent {
                throw Self.error(.unexpectedIndentation, next)
            }
            return value
        }

        /// A value written on one line: a scalar or a flow collection.
        static func inline(_ text: String, line: Int) throws(LocalhostError) -> YAMLValue {
            switch text.first {
            case "[":
                guard text.hasSuffix("]") else { throw error(.unsupportedYAML("flow sequences over several lines"), line: line) }
                let body = text.dropFirst().dropLast().trimmingCharacters(in: .whitespaces)
                if body.isEmpty { return YAMLValue(line: line, kind: .sequence([])) }
                var items: [YAMLValue] = []
                for item in try splitFlowItems(body, line: line) {
                    if item.first == "[" || item.first == "{" { throw error(.unsupportedYAML("nested flow collections"), line: line) }
                    items.append(YAMLValue(line: line, kind: .scalar(try scalar(item, line: line))))
                }
                return YAMLValue(line: line, kind: .sequence(items))
            case "{":
                guard text.dropFirst().dropLast().allSatisfy(\.isWhitespace), text.hasSuffix("}") else {
                    throw error(.unsupportedYAML("flow mappings ({…})"), line: line)
                }
                return YAMLValue(line: line, kind: .mapping([]))
            default:
                return YAMLValue(line: line, kind: .scalar(try scalar(text, line: line)))
            }
        }

        /// The text of a plain or quoted scalar.
        static func scalar(_ text: String, line: Int) throws(LocalhostError) -> String {
            switch text.first {
            case "\"":
                return try doubleQuoted(text, line: line)
            case "'":
                guard text.count >= 2, text.hasSuffix("'") else { throw error(.unterminatedQuote, line: line) }
                let body = String(text.dropFirst().dropLast())
                // Inside, a quote is written twice; a lone one would end the scalar early.
                guard !body.replacingOccurrences(of: "''", with: "").contains("'") else {
                    throw error(.unterminatedQuote, line: line)
                }
                return body.replacingOccurrences(of: "''", with: "'")
            case "|", ">":
                throw error(.unsupportedYAML("block scalars (| and >)"), line: line)
            case "&", "*":
                throw error(.unsupportedYAML("anchors and aliases"), line: line)
            case "!":
                throw error(.unsupportedYAML("tags"), line: line)
            default:
                return text
            }
        }

        static func doubleQuoted(_ text: String, line: Int) throws(LocalhostError) -> String {
            var result = ""
            var iterator = text.unicodeScalars.dropFirst().makeIterator()
            var isClosed = false
            while let scalar = iterator.next() {
                if isClosed { throw error(.unterminatedQuote, line: line) }
                switch scalar {
                case "\"":
                    isClosed = true
                case "\\":
                    guard let escape = iterator.next() else { throw error(.unterminatedQuote, line: line) }
                    switch escape {
                    case "\\", "\"", "/": result.unicodeScalars.append(escape)
                    case "n": result += "\n"
                    case "t": result += "\t"
                    case "r": result += "\r"
                    case "0": result += "\0"
                    case " ": result += " "
                    case "x", "u", "U":
                        let digits = escape == "x" ? 2 : escape == "u" ? 4 : 8
                        var hex = ""
                        for _ in 0..<digits {
                            guard let digit = iterator.next() else { throw error(.invalidEscape, line: line) }
                            hex.unicodeScalars.append(digit)
                        }
                        guard let code = UInt32(hex, radix: 16), let decoded = Unicode.Scalar(code) else {
                            throw error(.invalidEscape, line: line)
                        }
                        result.unicodeScalars.append(decoded)
                    default:
                        throw error(.invalidEscape, line: line)
                    }
                default:
                    result.unicodeScalars.append(scalar)
                }
            }
            guard isClosed else { throw error(.unterminatedQuote, line: line) }
            return result
        }

        /// The items of a flow sequence's body, split at commas outside quotes.
        static func splitFlowItems(_ body: String, line: Int) throws(LocalhostError) -> [String] {
            var items: [String] = []
            var current = ""
            var quote: Character?
            var isEscaped = false
            for character in body {
                if let open = quote {
                    if isEscaped {
                        isEscaped = false
                    } else if open == "\"" && character == "\\" {
                        isEscaped = true
                    } else if character == open {
                        quote = nil
                    }
                    current.append(character)
                } else if character == "," {
                    items.append(current.trimmingCharacters(in: .whitespaces))
                    current = ""
                } else {
                    if (character == "\"" || character == "'") && current.allSatisfy(\.isWhitespace) { quote = character }
                    current.append(character)
                }
            }
            guard quote == nil else { throw error(.unterminatedQuote, line: line) }
            items.append(current.trimmingCharacters(in: .whitespaces))
            // A trailing comma (`[a, b,]`) leaves an empty last item.
            if items.last == "" { items.removeLast() }
            guard !items.contains("") else { throw error(.expectedKeyValue, line: line) }
            return items
        }

        /// Splits `key: value` (or `key:`); a quoted key may contain `: `.
        static func keyValue(_ line: YAMLLines.Line) throws(LocalhostError) -> (key: String, rest: String) {
            let text = line.text
            var keyEnd: String.Index
            if let quote = text.first, quote == "\"" || quote == "'" {
                var index = text.index(after: text.startIndex)
                var isEscaped = false
                while index < text.endIndex {
                    let character = text[index]
                    if isEscaped {
                        isEscaped = false
                    } else if quote == "\"" && character == "\\" {
                        isEscaped = true
                    } else if character == quote {
                        // `''` inside single quotes is an escaped quote.
                        let next = text.index(after: index)
                        if quote == "'", next < text.endIndex, text[next] == "'" {
                            index = next
                        } else {
                            break
                        }
                    }
                    index = text.index(after: index)
                }
                guard index < text.endIndex else { throw error(.unterminatedQuote, line) }
                keyEnd = text.index(after: index)
                guard keyEnd < text.endIndex, text[keyEnd] == ":" else { throw error(.expectedKeyValue, line) }
            } else {
                guard let colon = text.range(of: ": ")?.lowerBound ?? (text.hasSuffix(":") ? text.index(before: text.endIndex) : nil)
                else { throw error(.expectedKeyValue, line) }
                keyEnd = colon
            }
            let rawKey = String(text[..<keyEnd]).trimmingCharacters(in: .whitespaces)
            guard !rawKey.isEmpty else { throw error(.expectedKeyValue, line) }
            if rawKey.first == "[" || rawKey.first == "{" || rawKey.first == "?" {
                throw error(.unsupportedYAML("complex keys"), line)
            }
            let key = try scalar(rawKey, line: line.number)
            let rest = String(text[text.index(after: keyEnd)...]).trimmingCharacters(in: .whitespaces)
            return (key, rest)
        }

        private static func error(_ problem: ProjectFileProblem, _ line: YAMLLines.Line, field: [String] = []) -> LocalhostError {
            .invalidProjectFile(problem, field: field, line: line.number)
        }

        private static func error(_ problem: ProjectFileProblem, line: Int) -> LocalhostError {
            .invalidProjectFile(problem, field: [], line: line)
        }

        private typealias Line = YAMLLines.Line
    }
}

private extension YAMLLines.Line {
    /// A line made of `text` alone, to test whether it reads as `key: value`.
    init(text: String) {
        self.init(number: 0, indent: 0, text: text)
    }
}

/// Writes scalars so that `YAMLValue` (and any YAML reader) reads back the
/// same text: plain when that is unambiguous, otherwise double-quoted.
enum YAMLScalar {
    static func write(_ text: String) -> String {
        isPlainSafe(text) ? text : doubleQuoted(text)
    }

    static func doubleQuoted(_ text: String) -> String {
        var result = "\""
        for scalar in text.unicodeScalars {
            switch scalar {
            case "\"": result += "\\\""
            case "\\": result += "\\\\"
            case "\n": result += "\\n"
            case "\t": result += "\\t"
            case "\r": result += "\\r"
            case _ where scalar.value < 0x20 || scalar.value == 0x7F:
                result += "\\x" + String(format: "%02X", scalar.value)
            default:
                result.unicodeScalars.append(scalar)
            }
        }
        return result + "\""
    }

    /// Whether `text` reads back as itself, as a string, without quotes.
    private static func isPlainSafe(_ text: String) -> Bool {
        guard let first = text.first, let last = text.last,
              !first.isWhitespace, !last.isWhitespace, last != ":",
              !"-?:,[]{}#&*!|>'\"%@`".contains(first),
              !text.contains(": "), !text.contains(" #"),
              !text.unicodeScalars.contains(where: { $0.value < 0x20 || $0.value == 0x7F })
        else { return false }
        return !looksLikeNonString(text)
    }

    /// Null, booleans and numbers, as YAML 1.1 or 1.2 readers would take them.
    private static func looksLikeNonString(_ text: String) -> Bool {
        let lowered = text.lowercased()
        let words: Set = ["~", "null", "true", "false", "yes", "no", "on", "off", "y", "n", ".inf", "-.inf", "+.inf", ".nan"]
        if words.contains(lowered) { return true }
        if Double(text) != nil { return true }
        guard let first = text.unicodeScalars.first else { return false }
        // Integers with prefixes, underscores or sexagesimal parts (`0x1F`, `1_000`, `12:30`).
        let startsNumeric = ("0"..."9").contains(first) || ((first == "+" || first == "." ) && text.count > 1)
        return startsNumeric && text.unicodeScalars.allSatisfy {
            ("0"..."9").contains($0) || ("a"..."f").contains($0) || ("A"..."F").contains($0) || "xXoObB_:.+-".unicodeScalars.contains($0)
        }
    }
}
