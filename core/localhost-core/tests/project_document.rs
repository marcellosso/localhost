//! Ported from `ProjectDocumentTests.swift`: Project files, writing, reading
//! with errors that name the line and field, and importing into the library
//! (Replace / Keep both).

mod common;

use std::sync::Arc;

use common::{Fixture, key};
use localhost_core::{
    ExecutionTarget, KeyClash, LocalhostError, MemorySecretStore, NewService, Origin, Project, ProjectChanges,
    ProjectDocument, ProjectFileProblem, ProjectLibrary, ServiceChanges, ValueKind, VariableInput, VariableLayer,
};

fn home() -> String {
    std::env::var("HOME").unwrap()
}

/// A Project using every field the file has: Services with Dependencies,
/// variables and Secrets on every layer, two Profiles, one active.
fn make_full_project(library: &ProjectLibrary) -> Project {
    let project = library
        .add_project("My SaaS", key("my-saas"), &format!("{}/Code/my-saas", home()), vec![], Origin::MainWindow)
        .unwrap();
    let changes = ProjectChanges {
        is_favorite: Some(true),
        variables: Some(vec![
            VariableInput::plain("LOG_LEVEL", "debug"),
            VariableInput::secret("SENTRY_DSN", "https://sentry.example/1"),
        ]),
        ..Default::default()
    };
    library.edit_project(project.id, changes, Origin::MainWindow).unwrap();
    let mut database = NewService::new("Database", key("database"), "docker compose up postgres");
    database.port = Some(5432);
    database.port_can_change = false;
    let database = library.add_service(project.id, database, vec![], vec![], Origin::MainWindow).unwrap();
    let mut api = NewService::new("API", key("api"), "uv run fastapi dev --port $PORT");
    api.working_directory = "./api/".into();
    api.port = Some(8000);
    let api = library
        .add_service(
            project.id,
            api,
            vec![
                VariableInput::plain("DATABASE_URL", "postgres://localhost:5432/app"),
                VariableInput::secret("STRIPE_KEY", "sk_test_123"),
            ],
            vec![database.id],
            Origin::MainWindow,
        )
        .unwrap();
    let mut web = NewService::new("Web", key("web"), "pnpm dev");
    web.working_directory = "web".into();
    web.is_excluded_from_start_all = true;
    web.port = Some(3000);
    web.url = Some("http://localhost:3000/admin".into());
    library.add_service(project.id, web, vec![], vec![api.id], Origin::MainWindow).unwrap();
    let development = library
        .add_profile(
            project.id,
            "Development",
            vec![VariableInput::plain("NODE_ENV", "development")],
            Origin::MainWindow,
        )
        .unwrap();
    library.add_profile(project.id, "Testing", vec![], Origin::MainWindow).unwrap();
    library.set_active_profile(Some(development.id), project.id, Origin::MainWindow).unwrap();
    library.project(project.id).unwrap()
}

// MARK: Writing

#[test]
fn writes_every_field_with_the_home_folder_as_a_tilde_and_secrets_by_name_only() {
    let fixture = Fixture::new();
    let project = make_full_project(&fixture.library());

    let yaml = ProjectDocument::from(&project).to_yaml();

    assert_eq!(
        yaml,
        "\
localhost: 1
name: My SaaS
key: my-saas
path: ~/Code/my-saas
variables:
  LOG_LEVEL: debug
secrets:
  - SENTRY_DSN
services:
  database:
    name: Database
    command: docker compose up postgres
    cwd: .
    port: 5432
    port_can_change: false
  api:
    name: API
    command: uv run fastapi dev --port $PORT
    cwd: api
    port: 8000
    depends_on:
      - database
    variables:
      DATABASE_URL: postgres://localhost:5432/app
    secrets:
      - STRIPE_KEY
  web:
    name: Web
    command: pnpm dev
    cwd: web
    port: 3000
    url: http://localhost:3000/admin
    start_all: false
    depends_on:
      - api
profiles:
  Development:
    variables:
      NODE_ENV: development
  Testing: {}
active_profile: Development
"
    );
    assert!(!yaml.contains("sk_test_123"));
    assert!(!yaml.contains("sentry.example"));
}

// MARK: Round trip

#[test]
fn export_import_export_gives_the_same_file_and_the_same_project_but_for_identity_and_secret_values() {
    let fixture = Fixture::new();
    let original = make_full_project(&fixture.library());
    let exported = ProjectDocument::from(&original).to_yaml();

    // Import on another machine: a fresh library.
    let other_root = tempfile::tempdir().unwrap();
    let other = ProjectLibrary::open(other_root.path(), Arc::new(MemorySecretStore::new())).unwrap();
    let imported =
        other.import_project(&ProjectDocument::from_yaml(&exported).unwrap(), None, Origin::MainWindow).unwrap();

    assert_eq!(ProjectDocument::from(&imported).to_yaml(), exported);
    assert_ne!(imported.id, original.id);
    assert!(!imported.is_favorite);
    assert_eq!(ProjectDocument::from(&imported), ProjectDocument::from(&original));
    let api = imported.services.iter().find(|service| service.key.as_str() == "api").unwrap();
    assert_eq!(api.secret_names, ["STRIPE_KEY"]);
    assert!(!other.is_secret_set("STRIPE_KEY", VariableLayer::Service(api.id)).unwrap());
    assert_eq!(imported.active_profile().map(|profile| profile.name.as_str()), Some("Development"));
    let dependency_keys: Vec<Vec<&str>> = imported
        .services
        .iter()
        .map(|service| {
            service.dependencies.iter().filter_map(|id| imported.service(*id)).map(|s| s.key.as_str()).collect()
        })
        .collect();
    assert_eq!(dependency_keys, [vec![], vec!["database"], vec!["api"]]);
}

// MARK: Reading

#[test]
fn reads_a_hand_written_file_like_the_specs_example() {
    let document = ProjectDocument::from_yaml(
        r#"# Written by hand.
localhost: 1
name: "My SaaS"
key: my-saas
path: ~/Code/my-saas

services:
  web:
    command: pnpm dev   # the Vite app
    cwd: ./web/
    port: "3000"
  api:
    command: 'uv run fastapi dev'
    cwd: ./api
    port: 8000
    depends_on: [database, database]
    variables:
      DEBUG: true
      EMPTY:
  database:
    command: docker compose up postgres
    port_can_change: false
"#,
    )
    .unwrap();

    assert_eq!(document.name(), "My SaaS");
    assert_eq!(document.path(), format!("{}/Code/my-saas", home()));
    let services = document.services();
    assert_eq!(services.iter().map(|service| service.name.as_str()).collect::<Vec<_>>(), ["web", "api", "database"]);
    assert_eq!(services[0].working_directory, "web");
    assert_eq!(services[0].port, Some(3000));
    assert_eq!(services[1].dependencies, [key("database")]);
    assert_eq!(
        services[1].variables.values,
        [("DEBUG".to_string(), "true".to_string()), ("EMPTY".to_string(), String::new())].into()
    );
    assert_eq!(services[2].working_directory, ".");
    assert!(!services[2].port_can_change);
    assert!(document.profiles().is_empty());
}

#[test]
fn rejects_an_invalid_file_naming_the_line_and_field() {
    use LocalhostError as E;
    use ProjectFileProblem as P;
    let invalid = |error: LocalhostError| P::Invalid(Box::new(error));
    let cases: Vec<(&str, P, &[&str], Option<usize>)> = vec![
        ("name: x\n", P::NotAProjectFile, &[], None),
        ("localhost: 2\nname: x\n", P::NewerFormat(2), &["localhost"], Some(1)),
        ("localhost: one\n", P::Expected(ValueKind::Number), &["localhost"], Some(1)),
        ("localhost: 1\nkey: a\npath: /a\n", P::Missing, &["name"], Some(1)),
        ("localhost: 1\nname: A\nkey: A B\npath: /a\n", invalid(E::InvalidKey("A B".into())), &["key"], Some(3)),
        ("localhost: 1\nname: A\nkey: a\npath: a\n", invalid(E::InvalidProjectPath("a".into())), &["path"], Some(4)),
        ("localhost: 1\nname: A\nkey: a\npath: /a\ncolour: red\n", P::UnknownField, &["colour"], Some(5)),
        ("localhost: 1\nname: A\nname: B\n", P::Duplicate, &["name"], Some(3)),
        ("localhost: 1\nname: |\n  A\n", P::UnsupportedYaml("block scalars (| and >)".into()), &[], Some(2)),
        ("localhost: 1\nname: \"A\n", P::UnterminatedQuote, &[], Some(2)),
        ("localhost: 1\nname: A\n   key: a\n", P::UnexpectedIndentation, &[], Some(3)),
        ("localhost: 1\njust words\n", P::ExpectedKeyValue, &[], Some(2)),
        (
            "localhost: 1\nname: A\nkey: a\npath: /a\nservices:\n  web:\n    cwd: web\n",
            P::Missing,
            &["services", "web", "command"],
            Some(6),
        ),
        (
            "localhost: 1\nname: A\nkey: a\npath: /a\nservices:\n  Web:\n    command: x\n",
            invalid(E::InvalidKey("Web".into())),
            &["services", "Web"],
            Some(6),
        ),
        (
            "localhost: 1\nname: A\nkey: a\npath: /a\nservices:\n  web:\n    command: x\n    port: 70000\n",
            invalid(E::InvalidPort(70000)),
            &["services", "web", "port"],
            Some(8),
        ),
        (
            "localhost: 1\nname: A\nkey: a\npath: /a\nservices:\n  web:\n    command: x\n    port: auto\n",
            P::Expected(ValueKind::Number),
            &["services", "web", "port"],
            Some(8),
        ),
        (
            "localhost: 1\nname: A\nkey: a\npath: /a\nservices:\n  web:\n    command: x\n    cwd: /abs\n",
            invalid(E::InvalidWorkingDirectory("/abs".into())),
            &["services", "web", "cwd"],
            Some(8),
        ),
        (
            "localhost: 1\nname: A\nkey: a\npath: /a\nservices:\n  web:\n    command: x\n    start_all: no\n",
            P::Expected(ValueKind::Boolean),
            &["services", "web", "start_all"],
            Some(8),
        ),
        (
            "localhost: 1\nname: A\nkey: a\npath: /a\nservices:\n  web:\n    command: x\n    depends_on:\n      - db\n",
            P::UnknownDependency("db".into()),
            &["services", "web", "depends_on"],
            Some(9),
        ),
        (
            "localhost: 1\nname: A\nkey: a\npath: /a\nservices:\n  web:\n    command: x\n    depends_on: [api]\n  api:\n    command: y\n    depends_on: [web]\n",
            invalid(E::DependencyCycle(vec![key("api"), key("web"), key("api")])),
            &["services", "api", "depends_on"],
            Some(11),
        ),
        (
            "localhost: 1\nname: A\nkey: a\npath: /a\nvariables:\n  9LIVES: x\n",
            invalid(E::InvalidEnvironmentVariableName("9LIVES".into())),
            &["variables", "9LIVES"],
            Some(6),
        ),
        (
            "localhost: 1\nname: A\nkey: a\npath: /a\nvariables:\n  TOKEN: x\nsecrets:\n  - TOKEN\n",
            invalid(E::DuplicateVariableName("TOKEN".into())),
            &["secrets"],
            Some(8),
        ),
        (
            "localhost: 1\nname: A\nkey: a\npath: /a\nprofiles:\n  Dev: {}\nactive_profile: Prod\n",
            P::UnknownProfile("Prod".into()),
            &["active_profile"],
            Some(7),
        ),
        (
            "localhost: 1\nname: A\nkey: a\npath: /a\nprofiles:\n  Dev:\n  \" Dev\": {}\n",
            invalid(E::DuplicateProfileName("Dev".into())),
            &["profiles", " Dev"],
            Some(7),
        ),
    ];
    for (yaml, problem, field, line) in cases {
        let expected =
            LocalhostError::InvalidProjectFile { problem, field: field.iter().map(|f| f.to_string()).collect(), line };
        assert_eq!(ProjectDocument::from_yaml(yaml), Err(expected), "{yaml}");
    }
}

#[test]
fn says_where_the_problem_is_in_its_message() {
    let error = LocalhostError::InvalidProjectFile {
        problem: ProjectFileProblem::Invalid(Box::new(LocalhostError::InvalidPort(70000))),
        field: vec!["services".into(), "web".into(), "port".into()],
        line: Some(8),
    };
    assert_eq!(
        error.message(),
        "Line 8, services › web › port: 70000 is not a valid port. Use a number from 1 to 65535."
    );
}

// MARK: Importing

#[test]
fn importing_a_key_that_is_taken_changes_nothing_unless_told_what_to_do() {
    let fixture = Fixture::new();
    let library = fixture.library();
    let existing = make_full_project(&library);
    let document = ProjectDocument::from_yaml(&ProjectDocument::from(&existing).to_yaml()).unwrap();

    assert_eq!(
        library.import_project(&document, None, Origin::MainWindow),
        Err(LocalhostError::DuplicateProjectKey(key("my-saas")))
    );
    assert_eq!(library.projects(), [existing]);
}

#[test]
fn keep_both_adds_the_import_under_the_next_free_key() {
    let fixture = Fixture::new();
    let library = fixture.library();
    let existing = make_full_project(&library);
    library.add_project("Taken", key("my-saas-2"), "/tmp/taken", vec![], Origin::MainWindow).unwrap();

    let imported = library
        .import_project(&ProjectDocument::from(&existing), Some(KeyClash::KeepBoth), Origin::MainWindow)
        .unwrap();

    assert_eq!(imported.key, key("my-saas-3"));
    assert_ne!(imported.id, existing.id);
    assert_eq!(library.project(existing.id), Ok(existing));
    assert_eq!(library.projects().len(), 3);
}

#[test]
fn replace_keeps_the_projects_identity_matching_services_and_profiles_and_their_secret_values() {
    let fixture = Fixture::new();
    let library = fixture.library();
    let existing = make_full_project(&library);
    let api = existing.services.iter().find(|service| service.key.as_str() == "api").unwrap().clone();
    let web = existing.services.iter().find(|service| service.key.as_str() == "web").unwrap().clone();
    let changes = ServiceChanges {
        variables: Some(vec![VariableInput::secret("SESSION_SECRET", "s3cret")]),
        ..Default::default()
    };
    library.edit_service(web.id, changes, Origin::MainWindow).unwrap();

    // The file drops `web` and the Project's Secret, renames the Project and adds a Service.
    let yaml = ProjectDocument::from(&existing)
        .to_yaml()
        .replace("name: My SaaS", "name: My SaaS (shared)")
        .replace("secrets:\n  - SENTRY_DSN\n", "")
        .replace(
            "  web:\n    name: Web\n    command: pnpm dev\n    cwd: web\n    port: 3000\n    url: http://localhost:3000/admin\n    start_all: false\n    depends_on:\n      - api\n",
            "  worker:\n    command: uv run worker\n",
        );
    let replaced = library
        .import_project(&ProjectDocument::from_yaml(&yaml).unwrap(), Some(KeyClash::Replace), Origin::MainWindow)
        .unwrap();

    assert_eq!(replaced.id, existing.id);
    assert_eq!(replaced.sort_order, existing.sort_order);
    assert!(replaced.is_favorite);
    assert_eq!(replaced.name, "My SaaS (shared)");
    let keys: Vec<&str> = replaced.services.iter().map(|service| service.key.as_str()).collect();
    assert_eq!(keys, ["database", "api", "worker"]);
    assert_eq!(replaced.services[1].id, api.id);
    let profile_ids = |project: &Project| project.profiles.iter().map(|profile| profile.id).collect::<Vec<_>>();
    assert_eq!(profile_ids(&replaced), profile_ids(&existing));
    assert_eq!(replaced.active_profile_id, existing.active_profile_id);
    assert_eq!(library.secret_value("STRIPE_KEY", VariableLayer::Service(api.id)), Ok(Some("sk_test_123".into())));
    // Secrets the import no longer names are removed.
    assert_eq!(fixture.secrets.accounts(), [VariableLayer::Service(api.id).secret_account("STRIPE_KEY")]);
    assert_eq!(library.projects(), std::slice::from_ref(&replaced));
    assert_eq!(fixture.library().projects(), [replaced]);
}

#[test]
fn a_project_in_a_wsl_distro_names_it_and_keeps_its_path_as_is() {
    let fixture = Fixture::new();
    let library = fixture.library();
    let wsl = ExecutionTarget::Wsl { distro: "Ubuntu-24.04".into() };
    let project =
        library.add_project_on(wsl.clone(), "Shop", key("shop"), "/home/dev/shop", vec![], Origin::MainWindow).unwrap();
    let text = ProjectDocument::from(&project).to_yaml();
    assert_eq!(text, "localhost: 1\nname: Shop\nkey: shop\nwsl: Ubuntu-24.04\npath: /home/dev/shop\n");

    let mut document = ProjectDocument::from_yaml(&text).unwrap();
    assert_eq!(document.execution_target(), &wsl);
    // Imported on a machine without WSL: this machine, in a folder picked there.
    document.set_execution_target(ExecutionTarget::ThisMachine);
    document.set_path("/tmp/shop");
    let imported = library.import_project(&document, Some(KeyClash::KeepBoth), Origin::MainWindow).unwrap();
    assert_eq!((imported.execution_target, imported.path.as_str()), (ExecutionTarget::ThisMachine, "/tmp/shop"));
}

#[test]
fn rejects_a_wsl_distro_name_that_cant_be_one() {
    let error = ProjectDocument::from_yaml("localhost: 1\nname: A\nkey: a\nwsl: my distro\npath: /a\n").unwrap_err();
    assert_eq!(error.message(), "Line 4, wsl: “my distro” is not the name of a WSL distro.");
}

#[test]
fn imports_into_the_folder_the_review_picked() {
    let fixture = Fixture::new();
    let mut document =
        ProjectDocument::from_yaml("localhost: 1\nname: A\nkey: a\npath: /Users/someone-else/a\n").unwrap();
    document.set_path("~/Elsewhere/a/");

    let imported = fixture.library().import_project(&document, None, Origin::MainWindow).unwrap();

    assert_eq!(imported.path, format!("{}/Elsewhere/a", home()));
}
