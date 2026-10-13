import Foundation

// Namespaces for the core's text functions, as the Swift core had them.

public typealias ANSIColor = AnsiColor

extension StyledLine {
    public typealias Span = LocalhostCore.Span

    public var plainText: String { spans.map(\.text).joined() }
}

/// ANSI escape sequences in Service output.
public enum ANSI {
    /// The line's text in styled spans, escape sequences removed.
    public static func parse(_ line: String) -> StyledLine {
        ansiParse(line: line)
    }

    /// The line without escape sequences.
    public static func strip(_ line: String) -> String {
        ansiStrip(line: line)
    }
}

/// Scores how well a query matches a name: its letters in order, with bonuses
/// for word starts and runs (the command palette, the CLI).
public enum FuzzyMatch {
    /// `nil` if the letters of `query` don't all appear in `text`, in order.
    public static func score(_ query: String, in text: String) -> Int? {
        fuzzyScore(query: query, text: text).map(Int.init)
    }
}

/// Local URLs (`http://localhost:3000`, `0.0.0.0` as localhost) in output.
public enum LocalURL {
    public static func matches(in text: String) -> [(range: Range<String.Index>, url: URL)] {
        localUrlMatches(text: text).compactMap { match in
            let utf8 = text.utf8
            guard let start = utf8.index(utf8.startIndex, offsetBy: Int(match.start), limitedBy: utf8.endIndex),
                  let end = utf8.index(utf8.startIndex, offsetBy: Int(match.end), limitedBy: utf8.endIndex),
                  let url = URL(string: match.url)
            else { return nil }
            return (start..<end, url)
        }
    }

    public static func first(in text: String) -> URL? {
        localUrlFirst(text: text).flatMap(URL.init(string:))
    }
}
