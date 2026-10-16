use super::{ScannedDirectory, ServiceDetector, ServiceSuggestion};
use crate::yaml::{YamlLine, YamlLines, unquoted};

/// Suggests one Service per Compose service, run in the foreground
/// (`docker compose up <service>`) so Stop's SIGINT shuts it down cleanly.
/// Its Port is the first published host port; it can't move, since the port
/// is fixed in the Compose file.
pub struct ComposeDetector;

/// In the order Docker Compose picks them; only the first one present is used.
const FILE_NAMES: [&str; 4] = ["compose.yaml", "compose.yml", "docker-compose.yaml", "docker-compose.yml"];

impl ServiceDetector for ComposeDetector {
    fn suggestions(&self, directory: &ScannedDirectory) -> Vec<ServiceSuggestion> {
        let Some(file_name) = FILE_NAMES.iter().find(|name| directory.contains(name)) else { return Vec::new() };
        let Some(text) = directory.read(file_name) else { return Vec::new() };
        services(&text)
            .into_iter()
            .map(|service| {
                let detail = match &service.image {
                    Some(image) => format!("Compose service · {image}"),
                    None => "Compose service".into(),
                };
                let mut suggestion = ServiceSuggestion::new(
                    service.name.clone(),
                    &directory.path,
                    format!("docker compose up {}", directory.syntax.quoted(&service.name)),
                    true,
                    directory.relative_path(file_name),
                    detail,
                );
                suggestion.port = service.port;
                suggestion.port_can_change = false;
                suggestion
            })
            .collect()
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ComposeService {
    name: String,
    image: Option<String>,
    /// The first published host port.
    port: Option<u16>,
}

/// The Compose services of a Compose file, in file order.
fn services(text: &str) -> Vec<ComposeService> {
    let yaml = YamlLines::parse(text);
    let block = yaml.block("services");
    let Some(service_indent) = block.first().map(|line| line.indent) else { return Vec::new() };
    let mut services = Vec::new();
    let mut index = 0;
    while index < block.len() {
        let line = &block[index];
        index += 1;
        if line.indent != service_indent || line.is_list_item() {
            continue;
        }
        let Some((name, _)) = line.key_value() else { continue };
        let body_length = block[index..].iter().take_while(|line| line.indent > service_indent).count();
        services.push(service(name, &block[index..index + body_length]));
        index += body_length;
    }
    services
}

fn service(name: String, body: &[YamlLine]) -> ComposeService {
    let mut service = ComposeService { name, image: None, port: None };
    let Some(property_indent) = body.first().map(|line| line.indent) else { return service };
    for (offset, line) in body.iter().enumerate() {
        if line.indent != property_indent || line.is_list_item() {
            continue;
        }
        let Some((key, value)) = line.key_value() else { continue };
        let following_length = body[offset + 1..]
            .iter()
            .take_while(|l| l.indent > property_indent || (l.indent == property_indent && l.is_list_item()))
            .count();
        let following = &body[offset + 1..offset + 1 + following_length];
        match key.as_str() {
            "image" => service.image = Some(unquoted(&value).to_string()),
            "ports" => service.port = first_published_port(&value, following),
            _ => {}
        }
    }
    service
}

fn first_published_port(value: &str, following: &[YamlLine]) -> Option<u16> {
    if value.starts_with('[') || following.first().is_some_and(|line| line.key_value().is_none()) {
        return YamlLines::sequence(value, following).iter().find_map(|entry| published_port_short(entry));
    }
    // Long syntax: `- target: 80` / `published: "8080"` mappings.
    following
        .iter()
        .filter_map(YamlLine::key_value)
        .find(|(key, _)| key == "published")
        .and_then(|(_, value)| published_port_from_host_part(unquoted(&value)))
}

/// The host port of a short-syntax entry such as `5432:5432`,
/// `127.0.0.1:8080:80/tcp` or `${PORT:-3000}:3000`; `None` when only a
/// container port is given (Docker picks the host port).
fn published_port_short(entry: &str) -> Option<u16> {
    let substituted = substituting_defaults(entry);
    let without_protocol = substituted.split('/').next().unwrap_or_default();
    let parts: Vec<&str> = without_protocol.split(':').collect();
    match parts.len() {
        2 => published_port_from_host_part(parts[0]),
        3 => published_port_from_host_part(parts[1]),
        _ => None,
    }
}

fn published_port_from_host_part(part: &str) -> Option<u16> {
    let substituted = substituting_defaults(part);
    let first = substituted.split('-').next().unwrap_or_default();
    first.parse::<u16>().ok().filter(|port| *port >= 1)
}

/// Replaces `${VAR:-default}` / `${VAR-default}` with the default and
/// `${VAR}` with nothing.
fn substituting_defaults(text: &str) -> String {
    let mut result = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        result.push_str(&rest[..start]);
        let Some(end) = rest[start + 2..].find('}').map(|offset| start + 2 + offset) else {
            result.push_str(&rest[start..]);
            return result;
        };
        let expression = &rest[start + 2..end];
        if let Some(dash) = expression.find('-') {
            result.push_str(&expression[dash + 1..]);
        }
        rest = &rest[end + 1..];
    }
    result + rest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_published_ports_in_every_short_form() {
        assert_eq!(published_port_short("5432:5432"), Some(5432));
        assert_eq!(published_port_short("127.0.0.1:8080:80/tcp"), Some(8080));
        assert_eq!(published_port_short("${PORT:-3000}:3000"), Some(3000));
        assert_eq!(published_port_short("8000-8010:8000-8010"), Some(8000));
        assert_eq!(published_port_short("9000"), None);
        assert_eq!(published_port_short("${PORT}:3000"), None);
    }
}
