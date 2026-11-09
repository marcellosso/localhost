//! Shaping Suggestions before they are saved: one Service for a whole Compose
//! file, several Suggestions merged into one, and which Suggestions a
//! Project's Services already cover when it is scanned again.
//!
//! All of it reads command lines; nothing here changes how a Service runs.

use super::{ServiceSuggestion, suggestion_key};
use crate::Service;

/// What a plain `docker compose up` command line starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ComposeUp {
    /// Every service of the Compose file Docker picks in the working directory.
    All,
    /// Only these, in the order given.
    Services(Vec<String>),
}

/// What `command_line` starts, when it is `docker compose up` (or
/// `docker-compose up`) with nothing but flags and service names after it.
/// Anything else, such as `-f other.yaml` before `up` or a second command
/// after `&&`, is `None`: localhost doesn't guess what it runs.
pub(crate) fn compose_up(command_line: &str) -> Option<ComposeUp> {
    if command_line.contains(['&', '|', ';', '<', '>', '`', '\n']) || command_line.contains("$(") {
        return None;
    }
    let words: Vec<&str> = command_line.split_whitespace().collect();
    let rest = match words.as_slice() {
        ["docker", "compose", "up", rest @ ..] | ["docker-compose", "up", rest @ ..] => rest,
        _ => return None,
    };
    let services: Vec<String> = rest.iter().filter(|word| !word.starts_with('-')).map(|word| unquoted(word)).collect();
    Some(if services.is_empty() { ComposeUp::All } else { ComposeUp::Services(services) })
}

fn unquoted(word: &str) -> String {
    for quote in ['\'', '"'] {
        if let Some(inner) = word.strip_prefix(quote).and_then(|word| word.strip_suffix(quote)) {
            return inner.to_string();
        }
    }
    word.to_string()
}

/// The name of a Service running a whole Compose file: `Infra` at the
/// Project root, `Infra (backend)` for one in `services/backend`.
fn infra_name(working_directory: &str) -> String {
    match working_directory.rsplit('/').next() {
        Some(folder) if working_directory != "." && !folder.is_empty() => format!("Infra ({folder})"),
        _ => "Infra".into(),
    }
}

/// One Suggestion running a whole Compose file (`docker compose up`), from
/// its Suggestions of one Compose service each.
pub(super) fn compose_file(parts: &[ServiceSuggestion]) -> ServiceSuggestion {
    let services = union(parts.iter().map(|part| part.compose_services.clone()));
    let detail = format!("Compose file · {} services", services.len());
    grouped(parts, infra_name(&parts[0].working_directory), "docker compose up".into(), detail, services, false)
}

fn compose_command(services: &[String]) -> String {
    // Compose service names are `[a-zA-Z0-9][a-zA-Z0-9_.-]*`: never quoted.
    std::iter::once("docker compose up".to_string()).chain(services.iter().cloned()).collect::<Vec<_>>().join(" ")
}

/// Several Suggestions as one, for the review step's Merge.
///
/// When every one runs `docker compose up` in the same working directory,
/// the merged one runs them together: `docker compose up` if one of them ran
/// the whole file, else `docker compose up a b c` with the names in order,
/// so Compose services the user left out stay out. It is named `Infra`.
/// Otherwise it takes the first one's name and command line, for the user to
/// edit: localhost doesn't invent a way to run unrelated commands together.
///
/// The working directory is the first one's. The Port is kept only when
/// exactly one of them has a Port; with several, none is (no single port
/// says when they are all up). `None` when there is nothing to merge.
pub fn merge_suggestions(suggestions: &[ServiceSuggestion]) -> Option<ServiceSuggestion> {
    let first = suggestions.first()?;
    if suggestions.len() == 1 {
        return Some(first.clone());
    }
    let same_directory = suggestions.iter().all(|s| s.working_directory == first.working_directory);
    let compose: Option<Vec<ComposeUp>> =
        if same_directory { suggestions.iter().map(|s| compose_up(&s.command_line)).collect() } else { None };
    if let Some(compose) = compose {
        let whole_file = compose.contains(&ComposeUp::All);
        let services = union(suggestions.iter().zip(&compose).map(|(suggestion, up)| match up {
            ComposeUp::All => suggestion.compose_services.clone(),
            ComposeUp::Services(services) => services.clone(),
        }));
        let command_line = if whole_file { "docker compose up".to_string() } else { compose_command(&services) };
        let detail = format!("Compose services · {}", services.join(", "));
        return Some(grouped(suggestions, infra_name(&first.working_directory), command_line, detail, services, false));
    }
    let names: Vec<&str> = suggestions.iter().map(|s| s.name.as_str()).collect();
    let detail = format!("Merged · {}", names.join(", "));
    Some(grouped(suggestions, first.name.clone(), first.command_line.clone(), detail, Vec::new(), true))
}

fn grouped(
    parts: &[ServiceSuggestion],
    name: String,
    command_line: String,
    detail: String,
    compose_services: Vec<String>,
    port_can_change: bool,
) -> ServiceSuggestion {
    let with_port: Vec<&ServiceSuggestion> = parts.iter().filter(|part| part.port.is_some()).collect();
    let (port, port_can_change) = match with_port.as_slice() {
        [only] => (only.port, only.port_can_change),
        _ => (None, port_can_change),
    };
    let found_in = union(parts.iter().map(|part| vec![part.found_in.clone()])).join(", ");
    ServiceSuggestion {
        key: suggestion_key(&name),
        name,
        working_directory: parts[0].working_directory.clone(),
        command_line,
        port,
        port_can_change,
        is_recommended: true,
        found_in,
        detail,
        compose_services,
    }
}

/// Every item once, in first-seen order.
fn union(lists: impl IntoIterator<Item = Vec<String>>) -> Vec<String> {
    let mut all: Vec<String> = Vec::new();
    for item in lists.into_iter().flatten() {
        if !all.contains(&item) {
            all.push(item);
        }
    }
    all
}

/// Whether `service` already does what `suggestion` proposes, so scanning
/// the Project again doesn't offer it. In the same working directory:
///
/// - the same command line;
/// - a `docker compose up` that runs the whole file covers every Compose
///   suggestion, and `docker compose up a b` covers those for `a` and `b`;
/// - any `docker compose up` covers the suggestion to run the whole file:
///   the user chose which Compose services to run, and the ones left out are
///   still offered one by one.
pub(crate) fn is_covered_by(suggestion: &ServiceSuggestion, service: &Service) -> bool {
    if service.working_directory != suggestion.working_directory {
        return false;
    }
    if service.command_line == suggestion.command_line {
        return true;
    }
    match (compose_up(&service.command_line), compose_up(&suggestion.command_line)) {
        (Some(ComposeUp::All), Some(_)) | (Some(_), Some(ComposeUp::All)) => true,
        (Some(ComposeUp::Services(running)), Some(ComposeUp::Services(wanted))) => {
            wanted.iter().all(|service| running.contains(service))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_a_compose_up_command_line_starts() {
        let services = |names: &[&str]| Some(ComposeUp::Services(names.iter().map(|n| n.to_string()).collect()));
        assert_eq!(compose_up("docker compose up"), Some(ComposeUp::All));
        assert_eq!(compose_up("docker-compose up --build"), Some(ComposeUp::All));
        assert_eq!(compose_up("docker compose up postgres  'temporal-ui'"), services(&["postgres", "temporal-ui"]));
        assert_eq!(compose_up("docker compose up --wait db"), services(&["db"]));
        assert_eq!(compose_up("docker compose -f other.yaml up"), None);
        assert_eq!(compose_up("docker compose up db && echo up"), None);
        assert_eq!(compose_up("npm run dev"), None);
    }

    #[test]
    fn names_infra_after_its_folder() {
        assert_eq!(infra_name("."), "Infra");
        assert_eq!(infra_name("services/backend"), "Infra (backend)");
    }
}
