use super::manifest::{TomlDocument, shell_quoted};
use super::{ScannedDirectory, ServiceDetector, ServiceSuggestion};

/// Suggests `cargo run` for a Cargo package with a binary, and `cargo run
/// --bin <name>` for each of its other binaries. Library-only packages and
/// virtual workspace roots get none; workspace members are scanned as their
/// own folders.
pub struct CargoDetector;

impl ServiceDetector for CargoDetector {
    fn declared_directories(&self, root: &ScannedDirectory) -> Vec<String> {
        let Some(manifest) = root.read("Cargo.toml").map(|text| TomlDocument::parse(&text)) else { return Vec::new() };
        root.expanding_globs(&TomlDocument::strings(manifest.table("workspace").and_then(|t| t.values.get("members"))))
    }

    fn suggestions(&self, directory: &ScannedDirectory) -> Vec<ServiceSuggestion> {
        let Some(manifest) = directory.read("Cargo.toml").map(|text| TomlDocument::parse(&text)) else {
            return Vec::new();
        };
        let Some(package_name) = TomlDocument::string(manifest.table("package").and_then(|t| t.values.get("name")))
        else {
            return Vec::new();
        };

        let has_main_binary = directory.contains("src/main.rs");
        let bin_folder = directory.subdirectory("src/bin");
        let mut binaries: Vec<String> =
            manifest.tables_named("bin").filter_map(|table| TomlDocument::string(table.values.get("name"))).collect();
        binaries
            .extend(bin_folder.file_names().into_iter().filter_map(|name| name.strip_suffix(".rs").map(String::from)));
        binaries.extend(
            bin_folder
                .subdirectory_names()
                .into_iter()
                .filter(|name| directory.contains(&format!("src/bin/{name}/main.rs"))),
        );
        let mut unique: Vec<String> = Vec::new();
        for name in binaries {
            if name != package_name && !unique.contains(&name) {
                unique.push(name);
            }
        }

        let suggestion = |name: &str, command_line: String, is_recommended: bool| {
            let detail = if is_recommended && has_main_binary {
                format!("package \"{package_name}\"")
            } else {
                format!("binary \"{name}\"")
            };
            ServiceSuggestion::new(
                name,
                &directory.path,
                command_line,
                is_recommended,
                directory.relative_path("Cargo.toml"),
                detail,
            )
        };
        let mut suggestions = Vec::new();
        if has_main_binary {
            suggestions.push(suggestion(&package_name, "cargo run".into(), true));
        }
        for (index, binary) in unique.iter().enumerate() {
            let is_only_binary = !has_main_binary && index == 0;
            suggestions.push(suggestion(binary, format!("cargo run --bin {}", shell_quoted(binary)), is_only_binary));
        }
        suggestions
    }
}
