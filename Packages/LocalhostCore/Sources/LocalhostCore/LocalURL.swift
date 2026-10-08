import Foundation

/// Finds the URLs of servers on this machine in a line of output, e.g.
/// `Local: http://localhost:5173/` or `Listening on http://0.0.0.0:8000`.
public enum LocalURL {
    /// Each local URL in `text` (plain, without ANSI escapes) with its range.
    /// Wildcard hosts (`0.0.0.0`, `[::]`) become `localhost`, so the URL opens.
    public static func matches(in text: String) -> [(range: Range<String.Index>, url: URL)] {
        text.matches(of: pattern).compactMap { match in
            var range = match.range
            // Punctuation that ends a sentence is not part of the URL.
            while range.lowerBound < range.upperBound, ".,;:!?".contains(text[text.index(before: range.upperBound)]) {
                range = range.lowerBound..<text.index(before: range.upperBound)
            }
            var components = URLComponents(string: String(text[range]))
            if ["0.0.0.0", "::", "[::]"].contains(components?.host ?? "") { components?.host = "localhost" }
            return components?.url.map { (range, $0) }
        }
    }

    /// The first local URL in `text`.
    public static func first(in text: String) -> URL? {
        matches(in: text).first?.url
    }

    nonisolated(unsafe) private static let pattern =
        /https?:\/\/(?:localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1?\])(?::\d{1,5})?(?:\/[^\s"'<>`)\]]*)?/
            .ignoresCase()
}
