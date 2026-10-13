use crate::fold::fold;

/// Loose matching of typed text against names, e.g. for the command palette
/// and the CLI: every word of the query must appear in the text as letters
/// in order (`stweb` finds "Start web"), case- and accent-insensitively.
pub struct FuzzyMatch;

impl FuzzyMatch {
    /// How well `query` matches `text`, higher is better; `None` when some
    /// word of the query is not found. An empty query matches with 0.
    ///
    /// Letters at the start of a word and runs of consecutive letters score
    /// more, gaps score less, and a query word the text starts with scores most.
    pub fn score(query: &str, text: &str) -> Option<i64> {
        let haystack: Vec<char> = fold(text).chars().collect();
        let mut total = 0;
        for word in fold(query).split_whitespace() {
            let word: Vec<char> = word.chars().collect();
            total += best_score(&word, &haystack)?;
        }
        Some(total)
    }
}

fn is_word_start(index: usize, text: &[char]) -> bool {
    if index == 0 {
        return true;
    }
    let previous = text[index - 1];
    !previous.is_alphanumeric() || (previous.is_lowercase() && text[index].is_uppercase())
}

/// The best of the greedy matches starting at each occurrence of the word's
/// first letter.
fn best_score(word: &[char], text: &[char]) -> Option<i64> {
    let Some(&first) = word.first() else { return Some(0) };
    let mut best: Option<i64> = None;
    for start in (0..text.len()).filter(|&i| text[i] == first) {
        // A later start can't match if this one ran out of text.
        let Some(score) = greedy_score(word, text, start) else { break };
        best = Some(best.map_or(score, |best| best.max(score)));
    }
    best
}

fn greedy_score(word: &[char], text: &[char], start: usize) -> Option<i64> {
    let mut score = if start == 0 { 10 } else { 0 };
    let mut position = start;
    let mut previous: Option<usize> = None;
    for &character in word {
        while position < text.len() && text[position] != character {
            position += 1;
        }
        if position >= text.len() {
            return None;
        }
        score += 1;
        if is_word_start(position, text) {
            score += 5;
        }
        if let Some(previous) = previous {
            let gap = (position - previous - 1) as i64;
            score += if gap == 0 { 4 } else { -gap.min(3) };
        }
        previous = Some(position);
        position += 1;
    }
    Some(score)
}

#[cfg(test)]
mod tests {
    use super::FuzzyMatch;

    #[test]
    fn an_empty_query_matches_anything() {
        assert_eq!(FuzzyMatch::score("", "web"), Some(0));
        assert_eq!(FuzzyMatch::score("  ", "web"), Some(0));
    }

    #[test]
    fn every_word_must_be_found_as_letters_in_order() {
        assert!(FuzzyMatch::score("start web", "Start web · Acme").is_some());
        assert!(FuzzyMatch::score("stweb", "Start web").is_some());
        assert!(FuzzyMatch::score("web start", "Start web").is_some(), "words match independently");
        assert!(FuzzyMatch::score("stop web", "Start web").is_none());
        assert!(FuzzyMatch::score("bew", "web").is_none());
    }

    #[test]
    fn matching_ignores_case_and_accents() {
        assert!(FuzzyMatch::score("CAFE", "Café API").is_some());
    }

    #[test]
    fn prefixes_and_word_starts_beat_letters_scattered_inside_words() {
        let prefix = FuzzyMatch::score("web", "web").unwrap();
        let word_start = FuzzyMatch::score("web", "Open web").unwrap();
        let scattered = FuzzyMatch::score("web", "Sweden Bar").unwrap();
        assert!(prefix > word_start);
        assert!(word_start > scattered);
    }

    #[test]
    fn the_best_occurrence_is_used_not_the_first() {
        // The first "a" is inside "Data"; the word "api" later is a better match.
        assert_eq!(FuzzyMatch::score("api", "Data api"), FuzzyMatch::score("api", "Open api"));
    }

    #[test]
    fn an_occurrence_that_runs_out_of_text_keeps_the_earlier_match() {
        // "ab" matches at the first "a"; the second "a" has no "b" after it.
        assert!(FuzzyMatch::score("ab", "a b a").is_some());
    }

    #[test]
    fn initials_of_words_match() {
        let initials = FuzzyMatch::score("oib", "Open in Browser").unwrap();
        let inside = FuzzyMatch::score("oib", "Robin Bob").unwrap();
        assert!(initials > inside);
    }
}
