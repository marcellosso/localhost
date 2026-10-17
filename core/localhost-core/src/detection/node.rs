use std::collections::{BTreeSet, HashMap};

use super::{CommandSyntax, ScannedDirectory, ServiceDetector, ServiceSuggestion};
use crate::yaml::YamlLines;

/// Suggests a Service for each `package.json` script, run with the package
/// manager the lockfiles point to.
///
/// Every script is suggested; one long-running script per package (`dev`,
/// else `develop`, `start`, `serve`, `watch`) is checked by default. In a
/// workspace monorepo the packages are checked rather than the root's
/// orchestrating scripts (`turbo dev`), since localhost runs each package as
/// its own Service. When the script runs a known dev server, its default port
/// is suggested and, if the tool takes a port flag, `$PORT` (`%PORT%` on
/// Windows) is passed to it. On Windows cmd finds the package
/// managers' `.cmd` shims, so the command lines are otherwise the same.
pub struct NodeDetector;

impl ServiceDetector for NodeDetector {
    fn declared_directories(&self, root: &ScannedDirectory) -> Vec<String> {
        NodeWorkspaces::new(root).package_paths
    }

    fn suggestions(&self, directory: &ScannedDirectory) -> Vec<ServiceSuggestion> {
        let Some(manifest) = directory.read("package.json").and_then(|text| PackageManifest::parse(&text)) else {
            return Vec::new();
        };
        if manifest.scripts.is_empty() {
            return Vec::new();
        }
        let root = directory.self_and_ancestors().pop().expect("a directory has itself");
        let workspaces = NodeWorkspaces::new(&root);
        let is_workspace_root = directory.is_root() && !workspaces.package_paths.is_empty();
        let is_outside_workspaces = !directory.is_root()
            && !workspaces.patterns.is_empty()
            && !workspaces.package_paths.contains(&directory.path);
        let package_manager = PackageManager::detect(directory);

        let scripts: Vec<&Script> =
            manifest.scripts.iter().filter(|script| !is_lifecycle_script(&script.name, &manifest.scripts)).collect();
        let names: Vec<&str> = scripts.iter().map(|script| script.name.as_str()).collect();
        let primary = primary_script(&names);
        let base_name = manifest.unscoped_name().or_else(|| (!directory.is_root()).then(|| directory.name()));

        let suggestions: Vec<ServiceSuggestion> = scripts
            .iter()
            .map(|script| {
                let is_primary = Some(script.name.as_str()) == primary;
                let name = match (is_primary, &base_name) {
                    (true, Some(base)) => base.clone(),
                    (false, Some(base)) => format!("{base} {}", script.name),
                    (_, None) => script.name.clone(),
                };
                let server = DevServer::recognize(&script.body);
                let mut command_line = package_manager.run(&script.name, directory.syntax);
                if let Some(flag) = server.as_ref().and_then(|server| server.port_flag) {
                    command_line +=
                        &format!("{} {flag} {}", package_manager.forwarding_arguments(), directory.syntax.port());
                }
                let mut suggestion = ServiceSuggestion::new(
                    name,
                    &directory.path,
                    command_line,
                    is_primary && !is_workspace_root && !is_outside_workspaces,
                    directory.relative_path("package.json"),
                    format!("\"{}\": {}", script.name, script.body),
                );
                suggestion.port = server.as_ref().and_then(|server| server.port);
                suggestion.port_can_change = server.as_ref().is_none_or(|server| server.can_move);
                suggestion
            })
            .collect();
        let (recommended, rest): (Vec<_>, Vec<_>) = suggestions.into_iter().partition(|s| s.is_recommended);
        recommended.into_iter().chain(rest).collect()
    }
}

/// Scripts npm runs around installing and publishing, and `pre`/`post` hooks
/// of other scripts; never Services.
fn is_lifecycle_script(name: &str, scripts: &[Script]) -> bool {
    const LIFECYCLE: [&str; 13] = [
        "preinstall",
        "install",
        "postinstall",
        "prepare",
        "prepublish",
        "prepublishOnly",
        "prepack",
        "postpack",
        "publish",
        "postpublish",
        "preversion",
        "version",
        "postversion",
    ];
    if LIFECYCLE.contains(&name) {
        return true;
    }
    ["pre", "post"].iter().any(|prefix| {
        name.strip_prefix(prefix).is_some_and(|hooked| scripts.iter().any(|script| script.name == hooked))
    })
}

/// The script checked by default: the first of `dev`, `develop`, `start`,
/// `serve`, `watch` present by exact name, else the first `dev:…`-style one.
fn primary_script<'a>(names: &[&'a str]) -> Option<&'a str> {
    const PREFERRED: [&str; 5] = ["dev", "develop", "start", "serve", "watch"];
    if let Some(exact) = PREFERRED.iter().find_map(|p| names.iter().find(|name| *name == p)) {
        return Some(exact);
    }
    names
        .iter()
        .copied()
        .find(|name| PREFERRED.iter().any(|p| name.starts_with(&format!("{p}:")) || name.starts_with(&format!("{p}-"))))
}

struct Script {
    name: String,
    body: String,
}

/// The parts of a `package.json` detection reads.
struct PackageManifest {
    name: Option<String>,
    /// In file order.
    scripts: Vec<Script>,
    /// `workspaces` as an array, or `workspaces.packages`.
    workspaces: Vec<String>,
    /// The `packageManager` field, e.g. `pnpm@9.1.0`.
    package_manager: Option<String>,
}

impl PackageManifest {
    fn parse(text: &str) -> Option<PackageManifest> {
        let json: serde_json::Value = serde_json::from_str(text).ok()?;
        let object = json.as_object()?;
        let strings = |value: Option<&serde_json::Value>| -> Vec<String> {
            value
                .and_then(|value| value.as_array())
                .map(|items| items.iter().filter_map(|item| item.as_str().map(String::from)).collect())
                .unwrap_or_default()
        };
        let workspaces = match object.get("workspaces") {
            Some(serde_json::Value::Array(_)) => strings(object.get("workspaces")),
            Some(serde_json::Value::Object(workspaces)) => strings(workspaces.get("packages")),
            _ => Vec::new(),
        };
        let script_bodies: HashMap<String, String> = object
            .get("scripts")
            .and_then(|scripts| scripts.as_object())
            .map(|scripts| {
                scripts.iter().filter_map(|(name, body)| Some((name.clone(), body.as_str()?.to_string()))).collect()
            })
            .unwrap_or_default();
        // serde_json's map doesn't keep the file's order; recover it from the text.
        let scripts = script_order(text, &script_bodies)
            .into_iter()
            .map(|name| Script { body: script_bodies[&name].clone(), name })
            .collect();
        Some(PackageManifest {
            name: object.get("name").and_then(|name| name.as_str()).map(String::from),
            scripts,
            workspaces,
            package_manager: object.get("packageManager").and_then(|field| field.as_str()).map(String::from),
        })
    }

    /// `@acme/web` → `web`.
    fn unscoped_name(&self) -> Option<String> {
        let name = self.name.as_deref().filter(|name| !name.is_empty())?;
        name.rsplit('/').next().map(String::from)
    }
}

fn script_order(text: &str, scripts: &HashMap<String, String>) -> Vec<String> {
    let names: BTreeSet<&String> = scripts.keys().collect();
    let Some(start) = text.find("\"scripts\"") else { return names.into_iter().cloned().collect() };
    let mut order: Vec<String> = Vec::new();
    let mut rest = &text[start + "\"scripts\"".len()..];
    while order.len() < names.len() {
        let Some(open) = rest.find('"') else { break };
        let after_quote = &rest[open + 1..];
        let Some(close) = after_quote.find('"') else { break };
        let word = &after_quote[..close];
        rest = &after_quote[close + 1..];
        if scripts.contains_key(word) && !order.iter().any(|name| name == word) && rest.trim_start().starts_with(':') {
            order.push(word.to_string());
        }
    }
    for name in names {
        if !order.contains(name) {
            order.push(name.clone());
        }
    }
    order
}

/// The packages of a Node workspace monorepo, from `pnpm-workspace.yaml` or
/// the root `package.json`'s `workspaces`.
struct NodeWorkspaces {
    patterns: Vec<String>,
    /// Folders with a `package.json` matching the patterns, relative to the root.
    package_paths: Vec<String>,
}

impl NodeWorkspaces {
    fn new(root: &ScannedDirectory) -> NodeWorkspaces {
        let patterns = if let Some(text) = root.read("pnpm-workspace.yaml") {
            let yaml = YamlLines::parse(&text);
            yaml.lines
                .iter()
                .find_map(|line| {
                    (line.indent == 0).then(|| line.key_value()).flatten().filter(|(key, _)| key == "packages")
                })
                .map(|(_, value)| YamlLines::sequence(&value, yaml.block("packages")))
                .unwrap_or_default()
        } else {
            root.read("package.json")
                .and_then(|text| PackageManifest::parse(&text))
                .map(|m| m.workspaces)
                .unwrap_or_default()
        };
        let package_paths: BTreeSet<String> = root
            .expanding_globs(&patterns)
            .into_iter()
            .filter(|path| root.contains(&format!("{path}/package.json")))
            .collect();
        NodeWorkspaces { patterns, package_paths: package_paths.into_iter().collect() }
    }
}

/// The package manager a folder's scripts run with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PackageManager {
    Npm,
    Yarn,
    Pnpm,
    Bun,
}

impl PackageManager {
    fn name(&self) -> &'static str {
        match self {
            PackageManager::Npm => "npm",
            PackageManager::Yarn => "yarn",
            PackageManager::Pnpm => "pnpm",
            PackageManager::Bun => "bun",
        }
    }

    fn named(name: &str) -> Option<PackageManager> {
        [PackageManager::Npm, PackageManager::Yarn, PackageManager::Pnpm, PackageManager::Bun]
            .into_iter()
            .find(|manager| manager.name() == name)
    }

    /// The `packageManager` field of the nearest `package.json` that has one
    /// wins; otherwise the nearest lockfile, walking up to the Project root;
    /// npm when there is none.
    fn detect(directory: &ScannedDirectory) -> PackageManager {
        const LOCKFILES: [(&str, PackageManager); 6] = [
            ("pnpm-lock.yaml", PackageManager::Pnpm),
            ("yarn.lock", PackageManager::Yarn),
            ("bun.lock", PackageManager::Bun),
            ("bun.lockb", PackageManager::Bun),
            ("package-lock.json", PackageManager::Npm),
            ("npm-shrinkwrap.json", PackageManager::Npm),
        ];
        for folder in directory.self_and_ancestors() {
            let field = folder.read("package.json").and_then(|text| PackageManifest::parse(&text)?.package_manager);
            if let Some(manager) =
                field.as_deref().and_then(|field| field.split('@').next()).and_then(PackageManager::named)
            {
                return manager;
            }
            if let Some((_, manager)) = LOCKFILES.iter().find(|(lockfile, _)| folder.contains(lockfile)) {
                return *manager;
            }
        }
        PackageManager::Npm
    }

    fn run(&self, script: &str, syntax: CommandSyntax) -> String {
        format!("{} run {}", self.name(), syntax.quoted(script))
    }

    /// What separates the script from extra arguments: npm needs `--`.
    fn forwarding_arguments(&self) -> &'static str {
        if *self == PackageManager::Npm { " --" } else { "" }
    }
}

/// A dev server recognised from a script's command.
#[derive(Debug, PartialEq, Eq)]
struct DevServer {
    port: Option<u16>,
    /// The flag that sets its port, when the tool doesn't read `PORT` itself
    /// and the script doesn't already pick a port.
    port_flag: Option<&'static str>,
    /// Whether a Run can use another port: false when the script fixes a literal port.
    can_move: bool,
}

/// Tools whose dev server port is known: names, subcommands (`None` = none)
/// with their default port, and the port flag (`None` when the tool reads
/// `PORT` from the environment).
type Tool = (&'static [&'static str], &'static [(Option<&'static str>, u16)], Option<&'static str>);

const TOOLS: [Tool; 14] = [
    (&["next"], &[(Some("dev"), 3000), (Some("start"), 3000)], None),
    (&["nuxt", "nuxi"], &[(Some("dev"), 3000), (Some("preview"), 3000)], None),
    (&["react-scripts"], &[(Some("start"), 3000)], None),
    (&["vite"], &[(None, 5173), (Some("dev"), 5173), (Some("serve"), 5173), (Some("preview"), 4173)], Some("--port")),
    (&["astro"], &[(Some("dev"), 4321), (Some("start"), 4321), (Some("preview"), 4321)], Some("--port")),
    (&["ng"], &[(Some("serve"), 4200), (Some("s"), 4200)], Some("--port")),
    (&["storybook"], &[(Some("dev"), 6006)], Some("-p")),
    (&["start-storybook"], &[(None, 6006)], Some("-p")),
    (&["wrangler"], &[(Some("dev"), 8787)], Some("--port")),
    (&["gatsby"], &[(Some("develop"), 8000), (Some("serve"), 9000)], Some("-p")),
    (&["webpack"], &[(Some("serve"), 8080)], Some("--port")),
    (&["webpack-dev-server"], &[(None, 8080)], Some("--port")),
    (&["docusaurus"], &[(Some("start"), 3000)], Some("--port")),
    (&["expo"], &[(Some("start"), 8081)], Some("--port")),
];

/// Wrappers that run the command after them.
const WRAPPERS: [&str; 6] = ["cross-env", "npx", "pnpx", "bunx", "dotenv", "env"];

enum ExplicitPort {
    Literal(u16),
    Variable,
}

impl DevServer {
    /// The dev server a script starts, if it is a single command of a known tool.
    fn recognize(script: &str) -> Option<DevServer> {
        if ["&&", "||", ";", "|", "&"].iter().any(|separator| script.contains(separator)) {
            return None;
        }
        let all: Vec<&str> = script.split_whitespace().collect();
        let mut words: &[&str] = &all;
        while let Some(first) = words.first() {
            if first.contains('=') || WRAPPERS.contains(first) || *first == "--" {
                words = &words[1..];
            } else {
                break;
            }
        }
        let (_, subcommands, port_flag) =
            TOOLS.iter().find(|(names, _, _)| words.first().is_some_and(|w| names.contains(w)))?;
        let arguments = &words[1..];
        let subcommand = arguments.first().filter(|argument| !argument.starts_with('-')).copied();
        let default_port = subcommands.iter().find(|(name, _)| *name == subcommand).map(|(_, port)| *port)?;

        Some(match explicit_port(arguments) {
            Some(ExplicitPort::Literal(port)) => DevServer { port: Some(port), port_flag: None, can_move: false },
            Some(ExplicitPort::Variable) => DevServer { port: Some(default_port), port_flag: None, can_move: true },
            None => DevServer { port: Some(default_port), port_flag: *port_flag, can_move: true },
        })
    }
}

/// A port the script already passes: `--port 3001`, `-p=3001`, `--port $PORT`.
fn explicit_port(arguments: &[&str]) -> Option<ExplicitPort> {
    for (index, argument) in arguments.iter().enumerate() {
        let mut value: Option<&str> = None;
        for flag in ["--port", "-p"] {
            if *argument == flag && index + 1 < arguments.len() {
                value = Some(arguments[index + 1]);
            }
            if let Some(rest) = argument.strip_prefix(&format!("{flag}=")) {
                value = Some(rest);
            }
        }
        let Some(value) = value else { continue };
        if let Ok(port) = value.parse() {
            return Some(ExplicitPort::Literal(port));
        }
        if value.contains("PORT") {
            return Some(ExplicitPort::Variable);
        }
    }
    None
}
