//! Finds the Project or Service a typed name means: its Key, else its name,
//! else the best `FuzzyMatch` of either, as the app's command palette ranks.

use localhost_core::{FuzzyMatch, Key};

/// Something that can be named on the command line.
pub trait Named {
    fn key(&self) -> &Key;
    fn name(&self) -> &str;
}

impl Named for localhost_core::Project {
    fn key(&self) -> &Key {
        &self.key
    }

    fn name(&self) -> &str {
        &self.name
    }
}

impl Named for localhost_core::Service {
    fn key(&self) -> &Key {
        &self.key
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Unresolved<'a, T> {
    NoMatch,
    /// Equally good matches, in the candidates' order.
    Ambiguous(Vec<&'a T>),
}

pub fn resolve<'a, T: Named>(query: &str, candidates: &'a [T]) -> Result<&'a T, Unresolved<'a, T>> {
    if let Some(exact) = candidates.iter().find(|candidate| candidate.key().as_str().eq_ignore_ascii_case(query)) {
        return Ok(exact);
    }
    let named: Vec<&T> =
        candidates.iter().filter(|candidate| candidate.name().to_lowercase() == query.to_lowercase()).collect();
    if !named.is_empty() {
        return only(named);
    }
    let scored: Vec<(i64, &T)> = candidates
        .iter()
        .filter_map(|candidate| {
            let key = FuzzyMatch::score(query, candidate.key().as_str());
            let name = FuzzyMatch::score(query, candidate.name());
            Some((key.max(name)?, candidate))
        })
        .collect();
    let Some(best) = scored.iter().map(|(score, _)| *score).max() else { return Err(Unresolved::NoMatch) };
    only(scored.into_iter().filter(|(score, _)| *score == best).map(|(_, candidate)| candidate).collect())
}

fn only<T>(mut matches: Vec<&T>) -> Result<&T, Unresolved<'_, T>> {
    if matches.len() == 1 { Ok(matches.remove(0)) } else { Err(Unresolved::Ambiguous(matches)) }
}

/// What to tell the user when `query` names no `noun` (`Project`,
/// `Service`) or several. `scope` follows the noun, e.g. `of Acme Shop`.
pub fn describe<T: Named>(
    unresolved: &Unresolved<T>,
    query: &str,
    noun: &str,
    scope: &str,
    candidates: &[T],
) -> String {
    let what = if scope.is_empty() { noun.to_string() } else { format!("{noun} {scope}") };
    match unresolved {
        Unresolved::NoMatch if candidates.is_empty() => {
            format!("No {what} matches “{query}”: there are no {noun}s yet.")
        }
        Unresolved::NoMatch => {
            let keys: Vec<&str> = candidates.iter().map(|candidate| candidate.key().as_str()).collect();
            format!("No {what} matches “{query}”. {noun}s: {}.", keys.join(", "))
        }
        Unresolved::Ambiguous(matches) => {
            let names: Vec<String> =
                matches.iter().map(|candidate| format!("{} ({})", candidate.key(), candidate.name())).collect();
            format!("“{query}” matches more than one {noun}: {}. Use its Key.", names.join(", "))
        }
    }
}

/// The Project `query` names, or the message saying why there is none.
pub fn project(projects: &[localhost_core::Project], query: &str) -> Result<localhost_core::Project, String> {
    resolve(query, projects).cloned().map_err(|unresolved| describe(&unresolved, query, "Project", "", projects))
}

/// The Service of `project` that `query` names, or the message saying why there is none.
pub fn service<'a>(project: &'a localhost_core::Project, query: &str) -> Result<&'a localhost_core::Service, String> {
    let scope = format!("of {}", project.name);
    resolve(query, &project.services)
        .map_err(|unresolved| describe(&unresolved, query, "Service", &scope, &project.services))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    struct Item(Key, &'static str);

    impl Named for Item {
        fn key(&self) -> &Key {
            &self.0
        }

        fn name(&self) -> &str {
            self.1
        }
    }

    fn items(pairs: &[(&str, &'static str)]) -> Vec<Item> {
        pairs.iter().map(|(key, name)| Item(Key::new(*key).unwrap(), name)).collect()
    }

    fn resolved<'a>(query: &str, candidates: &'a [Item]) -> Option<&'a str> {
        resolve(query, candidates).ok().map(|item| item.0.as_str())
    }

    #[test]
    fn a_key_wins_over_any_other_match() {
        let candidates = items(&[("web-admin", "Web"), ("web", "Website")]);
        assert_eq!(resolved("web", &candidates), Some("web"));
        assert_eq!(resolved("WEB", &candidates), Some("web"));
    }

    #[test]
    fn then_a_name_ignoring_case() {
        let candidates = items(&[("shop", "Acme Shop"), ("acme", "Acme")]);
        assert_eq!(resolved("acme shop", &candidates), Some("shop"));
    }

    #[test]
    fn then_the_best_fuzzy_match_of_key_or_name() {
        let candidates = items(&[("acme-shop", "Acme Shop"), ("blog", "Blog")]);
        assert_eq!(resolved("ashop", &candidates), Some("acme-shop"));
        assert_eq!(resolved("Café", &items(&[("cafe", "Le Café")])), Some("cafe"));
        // A prefix beats letters scattered through another name.
        assert_eq!(resolved("api", &items(&[("rapid", "Rapid"), ("api-gateway", "Gateway")])), Some("api-gateway"));
    }

    #[test]
    fn equally_good_matches_are_ambiguous() {
        let candidates = items(&[("shop-one", "Shop One"), ("shop-two", "Shop Two"), ("blog", "Blog")]);
        let Err(Unresolved::Ambiguous(matches)) = resolve("shop", &candidates) else { panic!("ambiguous") };
        assert_eq!(matches, [&candidates[0], &candidates[1]]);

        let same_name = items(&[("a", "Same"), ("b", "Same")]);
        assert!(matches!(resolve("same", &same_name), Err(Unresolved::Ambiguous(matches)) if matches.len() == 2));
    }

    #[test]
    fn nothing_found_is_no_match() {
        assert_eq!(resolve("zzz", &items(&[("web", "Web")])), Err(Unresolved::NoMatch));
        assert_eq!(resolve("web", &items(&[])), Err(Unresolved::NoMatch));
    }

    #[test]
    fn describes_what_went_wrong_with_the_keys_to_use() {
        let candidates = items(&[("db", "Postgres"), ("web", "Web")]);
        assert_eq!(
            describe(&Unresolved::NoMatch, "zzz", "Service", "of Acme", &candidates),
            "No Service of Acme matches “zzz”. Services: db, web."
        );
        assert_eq!(
            describe(&Unresolved::Ambiguous(candidates.iter().collect()), "x", "Project", "", &candidates),
            "“x” matches more than one Project: db (Postgres), web (Web). Use its Key."
        );
        assert_eq!(
            describe(&Unresolved::NoMatch, "zzz", "Project", "", &items(&[])),
            "No Project matches “zzz”: there are no Projects yet."
        );
    }
}
