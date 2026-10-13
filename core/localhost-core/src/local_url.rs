use std::ops::Range;

/// Finds the URLs of servers on this machine in a line of output, e.g.
/// `Local: http://localhost:5173/` or `Listening on http://0.0.0.0:8000`.
pub struct LocalUrl;

/// The hosts that count as this machine. Wildcard hosts become `localhost`,
/// so the URL opens.
const HOSTS: [(&str, Option<&str>); 5] = [
    ("localhost", None),
    ("127.0.0.1", None),
    ("0.0.0.0", Some("localhost")),
    ("[::1]", None),
    ("[::]", Some("localhost")),
];

impl LocalUrl {
    /// Each local URL in `text` (plain, without ANSI escapes) with its byte
    /// range, as written apart from a wildcard host.
    pub fn matches(text: &str) -> Vec<(Range<usize>, String)> {
        let lower = text.to_ascii_lowercase();
        let mut matches = Vec::new();
        let mut from = 0;
        while let Some(offset) = lower[from..].find("http") {
            let start = from + offset;
            match Self::match_at(text, &lower, start) {
                Some((end, url)) => {
                    matches.push((start..end, url));
                    from = end;
                }
                None => from = start + 4,
            }
        }
        matches
    }

    /// The first local URL in `text`.
    pub fn first(text: &str) -> Option<String> {
        Self::matches(text).into_iter().next().map(|(_, url)| url)
    }

    /// `https?://<local host>(:port)?(/path)?` at `start`: where it ends and the URL.
    fn match_at(text: &str, lower: &str, start: usize) -> Option<(usize, String)> {
        let rest = &lower[start..];
        let scheme_end = start
            + if rest.starts_with("https://") {
                8
            } else if rest.starts_with("http://") {
                7
            } else {
                return None;
            };
        let (host, replacement) = HOSTS.iter().find(|(host, _)| lower[scheme_end..].starts_with(host))?;
        let host_end = scheme_end + host.len();
        let mut end = host_end;
        let bytes = text.as_bytes();
        // An optional port of one to five digits.
        if bytes.get(end) == Some(&b':') {
            let digits = bytes[end + 1..].iter().take(5).take_while(|b| b.is_ascii_digit()).count();
            if digits > 0 {
                end += 1 + digits;
            }
        }
        // An optional path, up to whitespace, a quote, a bracket or a closing parenthesis.
        if bytes.get(end) == Some(&b'/') {
            end += text[end..].find(|c: char| c.is_whitespace() || "\"'<>`)]".contains(c)).unwrap_or(text.len() - end);
        }
        // Punctuation that ends a sentence is not part of the URL.
        while end > host_end && ".,;:!?".contains(text[..end].chars().next_back()?) {
            end -= 1;
        }
        let url = match replacement {
            Some(local) => format!("{}{local}{}", &text[start..scheme_end], &text[host_end..end]),
            None => text[start..end].to_string(),
        };
        Some((end, url))
    }
}

#[cfg(test)]
mod tests {
    use super::LocalUrl;

    #[test]
    fn detects_local_urls_in_output() {
        for (line, expected) in [
            ("  ➜  Local:   http://localhost:5173/", "http://localhost:5173/"),
            ("Uvicorn running on http://0.0.0.0:8000 (Press CTRL+C to quit)", "http://localhost:8000"),
            ("Listening at http://127.0.0.1:3000.", "http://127.0.0.1:3000"),
            ("served on http://[::1]:4000/docs, ok", "http://[::1]:4000/docs"),
            ("bound to http://[::]:4000", "http://localhost:4000"),
            ("see HTTPS://LOCALHOST:8443/a?b=c", "HTTPS://LOCALHOST:8443/a?b=c"),
        ] {
            assert_eq!(LocalUrl::first(line).as_deref(), Some(expected), "{line}");
        }
    }

    #[test]
    fn ignores_urls_of_other_hosts() {
        assert_eq!(LocalUrl::first("docs at https://vitejs.dev and http://example.com:3000"), None);
        assert_eq!(LocalUrl::matches("http://localhost:1 and http://localhost:2").len(), 2);
    }

    #[test]
    fn ranges_cover_the_url_as_written() {
        let text = "at http://0.0.0.0:8000/x. done";
        let (range, url) = &LocalUrl::matches(text)[0];
        assert_eq!(&text[range.clone()], "http://0.0.0.0:8000/x");
        assert_eq!(url, "http://localhost:8000/x");
    }

    #[test]
    fn stops_at_a_closing_parenthesis_or_bracket() {
        assert_eq!(LocalUrl::first("(see http://localhost:3000/a)").as_deref(), Some("http://localhost:3000/a"));
        assert_eq!(LocalUrl::first("[http://localhost:3000]").as_deref(), Some("http://localhost:3000"));
    }
}
