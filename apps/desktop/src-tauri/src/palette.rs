//! Ranking for the command palette: the core's `FuzzyMatch`, so the palette
//! matches as the macOS one and the CLI do.

use localhost_core::FuzzyMatch;

/// How well `query` matches each of `texts`, higher is better; `None` where
/// some word of the query isn't found.
#[tauri::command]
#[specta::specta]
pub fn fuzzy_scores(query: String, texts: Vec<String>) -> Vec<Option<i32>> {
    texts
        .iter()
        .map(|text| FuzzyMatch::score(&query, text).map(|score| score.clamp(0, i32::MAX.into()) as i32))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_each_text_and_leaves_out_those_that_do_not_match() {
        let scores = fuzzy_scores("stweb".into(), vec!["Start Web".into(), "Stop API".into()]);
        assert!(scores[0].is_some_and(|score| score > 0));
        assert_eq!(scores[1], None);
    }
}
