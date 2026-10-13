use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Case- and accent-insensitive form of `text`, for comparing and slugging:
/// "Café API" → "cafe api".
pub(crate) fn fold(text: &str) -> String {
    text.nfd().filter(|c| !is_combining_mark(*c)).flat_map(char::to_lowercase).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn drops_case_and_accents() {
        assert_eq!(super::fold("Café ÉTÉ Ünïcode"), "cafe ete unicode");
    }
}
