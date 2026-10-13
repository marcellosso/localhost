//! The Rust core reads and writes the Swift core's storage (XP1, ADR-0007):
//! updating the macOS app to the Rust core must not lose a Project or a Secret.

mod common;

use std::path::Path;
use std::sync::Arc;

use common::key;
use localhost_core::{
    Id, MemorySecretStore, Origin, ProjectChanges, ProjectLibrary, SecretStore, ServiceChanges, VariableInput,
    VariableLayer,
};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/swift-schema-1");

fn copy_fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("projects")).unwrap();
    for entry in std::fs::read_dir(Path::new(FIXTURE).join("projects")).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), root.path().join("projects").join(entry.file_name())).unwrap();
    }
    root
}

fn id(raw: &str) -> Id {
    raw.parse().unwrap()
}

#[test]
fn loads_every_field_the_swift_core_wrote() {
    let root = copy_fixture();
    let library = ProjectLibrary::open(root.path(), Arc::new(MemorySecretStore::new())).unwrap();

    assert!(library.unreadable_files().is_empty());
    let projects = library.projects();
    assert_eq!(projects.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(), ["acme-shop", "empty"]);

    let shop = &projects[0];
    assert_eq!(shop.id, id("CB62D167-59CC-426A-9C05-BAC3C869DBCA"));
    assert_eq!(shop.name, "Acme Shop");
    assert_eq!(shop.path, "/Users/dev/Code/acme-shop");
    assert!(shop.is_favorite);
    assert_eq!(shop.variables.values.get("NODE_ENV").map(String::as_str), Some("development"));
    assert_eq!(shop.variables.secret_names, ["DB_PASSWORD"]);
    assert_eq!(shop.active_profile().unwrap().name, "Testing");
    assert_eq!(shop.profiles.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Testing", "Development"]);
    assert_eq!(shop.profiles[0].variables.secret_names, ["TOKEN"]);

    let [postgres, api, storybook] = &shop.services[..] else { panic!("three services") };
    assert_eq!((postgres.port, postgres.port_can_change), (Some(5432), false));
    assert_eq!(api.working_directory, "api");
    assert_eq!(api.command_line, "uv run fastapi dev --port $PORT");
    assert_eq!(api.dependencies, [postgres.id]);
    assert_eq!(api.url.as_deref(), Some("http://localhost:8000/docs"));
    assert_eq!(api.secret_names, ["STRIPE_KEY"]);
    assert_eq!(api.environment.get("DEBUG").map(String::as_str), Some("1"));
    assert_eq!((storybook.port, storybook.url.as_ref()), (None, None));
    assert!(storybook.is_excluded_from_start_all);
    assert_eq!(storybook.dependencies, [api.id]);
}

#[test]
fn finds_secrets_under_the_accounts_the_swift_core_used() {
    let root = copy_fixture();
    let secrets = Arc::new(MemorySecretStore::new());
    let accounts = std::fs::read_to_string(Path::new(FIXTURE).join("secret-accounts.txt")).unwrap();
    for account in accounts.lines() {
        secrets.set_value("stored by Swift", account).unwrap();
    }
    let library = ProjectLibrary::open(root.path(), secrets).unwrap();
    let shop = &library.projects()[0];

    for (name, layer) in [
        ("DB_PASSWORD", VariableLayer::Project(shop.id)),
        ("STRIPE_KEY", VariableLayer::Service(shop.services[1].id)),
        ("TOKEN", VariableLayer::Profile(shop.profiles[0].id)),
    ] {
        assert_eq!(library.secret_value(name, layer), Ok(Some("stored by Swift".into())), "{name}");
    }
}

#[test]
fn writes_files_the_swift_core_can_read_back_unchanged() {
    let root = copy_fixture();
    let library = ProjectLibrary::open(root.path(), Arc::new(MemorySecretStore::new())).unwrap();
    let shop_id = id("CB62D167-59CC-426A-9C05-BAC3C869DBCA");
    let file = root.path().join("projects/CB62D167-59CC-426A-9C05-BAC3C869DBCA.json");
    let original: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();

    // A change and its undo rewrite the file through the Rust core.
    library
        .edit_project(
            shop_id,
            ProjectChanges { name: Some("Renamed".into()), ..Default::default() },
            Origin::MainWindow,
        )
        .unwrap();
    library
        .edit_project(
            shop_id,
            ProjectChanges { name: Some("Acme Shop".into()), ..Default::default() },
            Origin::MainWindow,
        )
        .unwrap();

    let rewritten: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(rewritten, original, "same fields, same uppercase ids, omitted optionals still omitted");
    let text = std::fs::read_to_string(&file).unwrap();
    let keys: Vec<&str> = text.lines().filter_map(|line| line.trim().strip_prefix('"')?.split('"').next()).collect();
    assert_eq!(&keys[..2], ["project", "activeProfileID"], "keys are sorted, like the Swift files");
}

#[test]
fn new_ids_are_written_uppercase_in_file_names_and_secret_accounts() {
    let root = tempfile::tempdir().unwrap();
    let secrets = Arc::new(MemorySecretStore::new());
    let library = ProjectLibrary::open(root.path(), secrets.clone()).unwrap();
    let project = library.add_project("App", key("app"), "/tmp/app", vec![], Origin::MainWindow).unwrap();
    let service = library
        .add_service(
            project.id,
            localhost_core::NewService::new("Web", key("web"), "true"),
            vec![],
            vec![],
            Origin::MainWindow,
        )
        .unwrap();
    library
        .edit_service(
            service.id,
            ServiceChanges { variables: Some(vec![VariableInput::secret("TOKEN", "x")]), ..Default::default() },
            Origin::MainWindow,
        )
        .unwrap();

    let name = std::fs::read_dir(root.path().join("projects")).unwrap().next().unwrap().unwrap().file_name();
    let name = name.to_string_lossy();
    assert_eq!(name, name.to_uppercase().replace(".JSON", ".json"));
    let account = &secrets.accounts()[0];
    assert_eq!(account, &account.to_uppercase());
}
