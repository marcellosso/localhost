import Foundation

/// Loose matching of typed text against names, e.g. for the command palette:
/// every word of the query must appear in the text as letters in order
/// (`stweb` finds "Start web"), case- and accent-insensitively.
public enum FuzzyMatch {
    /// How well `query` matches `text`, higher is better; `nil` when some
    /// word of the query is not found. An empty query matches with 0.
    ///
    /// Letters at the start of a word and runs of consecutive letters score
    /// more, gaps score less, and a query word the text starts with scores most.
    public static func score(_ query: String, in text: String) -> Int? {
        let haystack = Array(fold(text))
        var total = 0
        for word in fold(query).split(whereSeparator: \.isWhitespace) {
            guard let score = bestScore(Array(word), in: haystack) else { return nil }
            total += score
        }
        return total
    }

    private static func fold(_ text: String) -> String {
        text.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: nil)
    }

    private static func isWordStart(_ index: Int, in text: [Character]) -> Bool {
        guard index > 0 else { return true }
        let previous = text[index - 1]
        return !previous.isLetter && !previous.isNumber || (previous.isLowercase && text[index].isUppercase)
    }

    /// The best of the greedy matches starting at each occurrence of the
    /// word's first letter.
    private static func bestScore(_ word: [Character], in text: [Character]) -> Int? {
        guard let first = word.first else { return 0 }
        var best: Int?
        for start in text.indices where text[start] == first {
            guard let score = greedyScore(word, in: text, from: start) else { break }
            best = max(best ?? .min, score)
        }
        return best
    }

    private static func greedyScore(_ word: [Character], in text: [Character], from start: Int) -> Int? {
        var score = start == 0 ? 10 : 0
        var position = start
        var previous: Int?
        for character in word {
            while position < text.count, text[position] != character { position += 1 }
            guard position < text.count else { return nil }
            score += 1
            if isWordStart(position, in: text) { score += 5 }
            if let previous {
                score += position == previous + 1 ? 4 : -min(position - previous - 1, 3)
            }
            previous = position
            position += 1
        }
        return score
    }
}
