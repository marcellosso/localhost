//! Ported from `ProjectLibraryTests.swift`.

mod common;

use std::os::unix::fs::PermissionsExt;

use common::{Fixture, key};
use localhost_core::{
    Id, LocalhostError, NewService, Origin, ProjectChanges, ProjectLibraryEvent, ServiceChanges, UnreadableProjectFile,
    VariableInput,
};

fn web(command_line: &str) -> NewService {
    NewService::new("Web", key("web"), command_line)
}

// MARK: Projects

#[test]
fn starts_empty() {
    assert!(Fixture::new().library().projects().is_empty());
}

#[test]
fn adds_a_project_and_persists_it_as_its_own_json_file() {
    let fixture = Fixture::new();
    let project = fixture
        .library()
        .add_project("  My SaaS ", key("my-saas"), "/Users/dev/Code/my-saas/", vec![], Origin::MainWindow)
        .unwrap();

    assert_eq!(project.name, "My SaaS");
    assert_eq!(project.path, "/Users/dev/Code/my-saas");
    assert!(project.services.is_empty());
    assert!(!project.is_favorite);

    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.project_file(project.id)).unwrap()).unwrap();
    assert_eq!(json["schemaVersion"], 1);
}

#[test]
fn reloads_exactly_what_was_saved() {
    let fixture = Fixture::new();
    let library = fixture.library();
    let project = library.add_project("App", key("app"), "/tmp/app", vec![], Origin::MainWindow).unwrap();
    let mut service = NewService::new("Web", key("web"), "pnpm dev");
    service.working_directory = "web".into();
    library
        .add_service(project.id, service, VariableInput::plain_all([("PORT", "3000")]), vec![], Origin::MainWindow)
        .unwrap();
    library
        .edit_project(project.id, ProjectChanges { is_favorite: Some(true), ..Default::default() }, Origin::MenuBar)
        .unwrap();

    assert_eq!(fixture.library().projects(), library.projects());
}

#[test]
fn adds_a_project_together_with_its_reviewed_services_in_one_save() {
    let fixture = Fixture::new();
    let mut web = NewService::new("Web", key("web"), "pnpm run dev --port $PORT");
    web.working_directory = "./apps/web/".into();
    web.port = Some(5173);
    let mut postgres = NewService::new("Postgres", key("postgres"), "docker compose up postgres");
    postgres.port = Some(5432);
    postgres.port_can_change = false;

    let project = fixture
        .library()
        .add_project("Shop", key("shop"), "/tmp/shop", vec![web, postgres], Origin::MainWindow)
        .unwrap();

    let keys: Vec<&str> = project.services.iter().map(|service| service.key.as_str()).collect();
    assert_eq!(keys, ["web", "postgres"]);
    assert_eq!(project.services[0].working_directory, "apps/web");
    assert_eq!(project.services[1].port, Some(5432));
    assert!(!project.services[1].port_can_change);
    assert_eq!(fixture.library().projects(), [project]);
}

#[test]
fn a_project_whose_services_are_invalid_is_not_added_at_all() {
    let fixture = Fixture::new();
    let library = fixture.library();
    let services = vec![web("pnpm dev"), NewService::new("Web again", key("web"), "pnpm start")];
    assert!(library.add_project("Shop", key("shop"), "/tmp/shop", services, Origin::MainWindow).is_err());
    assert!(library.projects().is_empty());
    assert!(fixture.library().projects().is_empty());
}

#[test]
fn adds_several_services_at_once_all_or_none() {
    let fixture = Fixture::new();
    let library = fixture.library();
    let project = library.add_project("App", key("app"), "/tmp/app", vec![], Origin::MainWindow).unwrap();
    library
        .add_service(project.id, NewService::new("API", key("api"), "make api"), vec![], vec![], Origin::MainWindow)
        .unwrap();

    assert_eq!(
        library.add_services(
            vec![web("pnpm dev"), NewService::new("API", key("api"), "uv run api")],
            project.id,
            Origin::MainWindow
        ),
        Err(LocalhostError::DuplicateServiceKey { key: key("api"), project_id: project.id })
    );
    assert_eq!(library.project(project.id).unwrap().services.len(), 1);

    let added = library.add_services(vec![web("pnpm dev")], project.id, Origin::MainWindow).unwrap();
    assert_eq!(added.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["web"]);
    let keys: Vec<String> = library.project(project.id).unwrap().services.iter().map(|s| s.key.to_string()).collect();
    assert_eq!(keys, ["api", "web"]);
}

#[test]
fn assigns_increasing_sort_orders_and_lists_projects_in_that_order() {
    let library = Fixture::new().library();
    let b = library.add_project("B", key("b"), "/tmp/b", vec![], Origin::MainWindow).unwrap();
    let a = library.add_project("A", key("a"), "/tmp/a", vec![], Origin::MainWindow).unwrap();

    assert!(b.sort_order < a.sort_order);
    assert_eq!(library.projects().iter().map(|p| p.id).collect::<Vec<_>>(), [b.id, a.id]);
}

#[test]
fn expands_a_tilde_in_the_project_path() {
    let project = Fixture::new().library().add_project("A", key("a"), "~/Code/a", vec![], Origin::MainWindow).unwrap();
    assert_eq!(project.path, format!("{}/Code/a", std::env::var("HOME").unwrap()));
}

#[test]
fn rejects_a_relative_project_path() {
    assert_eq!(
        Fixture::new().library().add_project("A", key("a"), "Code/a", vec![], Origin::MainWindow),
        Err(LocalhostError::InvalidProjectPath("Code/a".into()))
    );
}

#[test]
fn rejects_an_empty_name() {
    assert_eq!(
        Fixture::new().library().add_project("  ", key("a"), "/tmp/a", vec![], Origin::MainWindow),
        Err(LocalhostError::InvalidName)
    );
}

#[test]
fn project_keys_are_unique_app_wide() {
    let library = Fixture::new().library();
    library.add_project("One", key("app"), "/tmp/one", vec![], Origin::MainWindow).unwrap();
    let two = library.add_project("Two", key("two"), "/tmp/two", vec![], Origin::MainWindow).unwrap();

    assert_eq!(
        library.add_project("Three", key("app"), "/tmp/three", vec![], Origin::MainWindow),
        Err(LocalhostError::DuplicateProjectKey(key("app")))
    );
    assert_eq!(
        library.edit_project(
            two.id,
            ProjectChanges { key: Some(key("app")), ..Default::default() },
            Origin::MainWindow
        ),
        Err(LocalhostError::DuplicateProjectKey(key("app")))
    );
}

#[test]
fn editing_a_project_can_keep_its_own_key() {
    let library = Fixture::new().library();
    let project = library.add_project("App", key("app"), "/tmp/app", vec![], Origin::MainWindow).unwrap();
    let edited = library
        .edit_project(
            project.id,
            ProjectChanges { name: Some("Renamed".into()), key: Some(key("app")), ..Default::default() },
            Origin::MainWindow,
        )
        .unwrap();

    assert_eq!(edited.name, "Renamed");
    assert_eq!(edited.key, project.key);
}

#[test]
fn edits_only_the_fields_that_are_given() {
    let fixture = Fixture::new();
    let library = fixture.library();
    let project = library.add_project("App", key("app"), "/tmp/app", vec![], Origin::MainWindow).unwrap();
    let edited = library
        .edit_project(
            project.id,
            ProjectChanges { path: Some("/tmp/moved".into()), ..Default::default() },
            Origin::MainWindow,
        )
        .unwrap();

    assert_eq!(edited.path, "/tmp/moved");
    assert_eq!(edited.name, "App");
    assert_eq!(edited.key, project.key);
    assert_eq!(fixture.library().project(project.id), Ok(edited));
}

#[test]
fn removes_a_project_and_its_file() {
    let fixture = Fixture::new();
    let library = fixture.library();
    let project = library.add_project("App", key("app"), "/tmp/app", vec![], Origin::MainWindow).unwrap();
    library.remove_project(project.id, Origin::MainWindow).unwrap();

    assert!(library.projects().is_empty());
    assert!(!fixture.project_file(project.id).exists());
    assert_eq!(
        library.remove_project(project.id, Origin::MainWindow),
        Err(LocalhostError::ProjectNotFound(project.id))
    );
}

#[test]
fn free_project_keys_count_up() {
    let library = Fixture::new().library();
    assert_eq!(library.free_project_key(&key("app")), key("app"));
    library.add_project("App", key("app"), "/tmp/app", vec![], Origin::MainWindow).unwrap();
    library.add_project("App 2", key("app-2"), "/tmp/app2", vec![], Origin::MainWindow).unwrap();
    assert_eq!(library.free_project_key(&key("app")), key("app-3"));

    let long = key(&"a".repeat(64));
    library.add_project("Long", long.clone(), "/tmp/long", vec![], Origin::MainWindow).unwrap();
    assert_eq!(library.free_project_key(&long), key(&format!("{}-2", "a".repeat(60))));
}

// MARK: Services

#[test]
fn adds_edits_and_removes_services() {
    let fixture = Fixture::new();
    let library = fixture.library();
    let project = library.add_project("App", key("app"), "/tmp/app", vec![], Origin::MainWindow).unwrap();
    let mut api = NewService::new("API", key("api"), "  uv run fastapi dev\n");
    api.working_directory = "./api/".into();
    let api = library.add_service(project.id, api, vec![], vec![], Origin::MainWindow).unwrap();
    assert_eq!(api.working_directory, "api");
    assert_eq!(api.command_line, "uv run fastapi dev");

    let edited = library
        .edit_service(
            api.id,
            ServiceChanges {
                command_line: Some("uv run fastapi dev --port 8001".into()),
                variables: Some(VariableInput::plain_all([("DEBUG", "1")])),
                ..Default::default()
            },
            Origin::MainWindow,
        )
        .unwrap();
    assert_eq!(edited.name, "API");
    assert_eq!(edited.environment.get("DEBUG").map(String::as_str), Some("1"));
    assert_eq!(fixture.library().project(project.id).unwrap().services, [edited]);

    library.remove_service(api.id, Origin::MainWindow).unwrap();
    assert!(fixture.library().project(project.id).unwrap().services.is_empty());
    assert_eq!(library.remove_service(api.id, Origin::MainWindow), Err(LocalhostError::ServiceNotFound(api.id)));
}

#[test]
fn edits_and_clears_port_and_url() {
    let library = Fixture::new().library();
    let project = library.add_project("App", key("app"), "/tmp/app", vec![], Origin::MainWindow).unwrap();
    let service = library.add_service(project.id, web("pnpm dev"), vec![], vec![], Origin::MainWindow).unwrap();

    let set = ServiceChanges {
        port: Some(Some(5173)),
        url: Some(Some("http://localhost:5173".into())),
        ..Default::default()
    };
    let edited = library.edit_service(service.id, set, Origin::MainWindow).unwrap();
    assert_eq!((edited.port, edited.url.as_deref()), (Some(5173), Some("http://localhost:5173")));

    let cleared = library
        .edit_service(
            service.id,
            ServiceChanges { port: Some(None), url: Some(None), ..Default::default() },
            Origin::MainWindow,
        )
        .unwrap();
    assert_eq!((cleared.port, cleared.url), (None, None));

    assert_eq!(
        library.edit_service(
            service.id,
            ServiceChanges { port: Some(Some(0)), ..Default::default() },
            Origin::MainWindow
        ),
        Err(LocalhostError::InvalidPort(0))
    );
}

#[test]
fn service_keys_are_unique_per_project_not_app_wide() {
    let library = Fixture::new().library();
    let one = library.add_project("One", key("one"), "/tmp/one", vec![], Origin::MainWindow).unwrap();
    let two = library.add_project("Two", key("two"), "/tmp/two", vec![], Origin::MainWindow).unwrap();
    library.add_service(one.id, web("pnpm dev"), vec![], vec![], Origin::MainWindow).unwrap();
    library.add_service(two.id, web("pnpm dev"), vec![], vec![], Origin::MainWindow).unwrap();
    let api = library
        .add_service(one.id, NewService::new("API", key("api"), "make api"), vec![], vec![], Origin::MainWindow)
        .unwrap();

    let duplicate = Some(LocalhostError::DuplicateServiceKey { key: key("web"), project_id: one.id });
    assert_eq!(
        library
            .add_service(one.id, NewService::new("Web 2", key("web"), "pnpm dev"), vec![], vec![], Origin::MainWindow)
            .err(),
        duplicate
    );
    assert_eq!(
        library
            .edit_service(api.id, ServiceChanges { key: Some(key("web")), ..Default::default() }, Origin::MainWindow)
            .err(),
        duplicate
    );
}

#[test]
fn an_empty_working_directory_means_the_project_root() {
    for raw in ["", ".", "./", " . "] {
        let library = Fixture::new().library();
        let project = library.add_project("App", key("app"), "/tmp/app", vec![], Origin::MainWindow).unwrap();
        let mut service = NewService::new("All", key("all"), "make");
        service.working_directory = raw.into();
        let service = library.add_service(project.id, service, vec![], vec![], Origin::MainWindow).unwrap();
        assert_eq!(service.working_directory, ".", "{raw:?}");
        assert_eq!(project.working_directory(&service), std::path::PathBuf::from("/tmp/app"));
    }
}

#[test]
fn working_directories_resolve_against_the_project_path() {
    let library = Fixture::new().library();
    let project = library.add_project("App", key("app"), "/tmp/app", vec![], Origin::MainWindow).unwrap();
    let mut service = NewService::new("Shared", key("shared"), "make");
    service.working_directory = "../shared/tools".into();
    let service = library.add_service(project.id, service, vec![], vec![], Origin::MainWindow).unwrap();
    assert_eq!(project.working_directory(&service), std::path::PathBuf::from("/tmp/shared/tools"));
}

#[test]
fn rejects_an_absolute_working_directory() {
    for raw in ["/tmp/app/web", "~/web"] {
        let library = Fixture::new().library();
        let project = library.add_project("App", key("app"), "/tmp/app", vec![], Origin::MainWindow).unwrap();
        let mut service = web("pnpm dev");
        service.working_directory = raw.into();
        assert_eq!(
            library.add_service(project.id, service, vec![], vec![], Origin::MainWindow),
            Err(LocalhostError::InvalidWorkingDirectory(raw.into()))
        );
    }
}

#[test]
fn rejects_an_empty_command_line() {
    let library = Fixture::new().library();
    let project = library.add_project("App", key("app"), "/tmp/app", vec![], Origin::MainWindow).unwrap();
    assert_eq!(
        library.add_service(project.id, web(" \n"), vec![], vec![], Origin::MainWindow),
        Err(LocalhostError::EmptyCommandLine)
    );
}

#[test]
fn rejects_invalid_environment_variable_names() {
    for name in ["", "1PORT", "MY-VAR", "MY VAR", "ÉTÉ"] {
        let library = Fixture::new().library();
        let project = library.add_project("App", key("app"), "/tmp/app", vec![], Origin::MainWindow).unwrap();
        assert_eq!(
            library.add_service(
                project.id,
                web("pnpm dev"),
                VariableInput::plain_all([(name, "x"), ("_OK_1", "y")]),
                vec![],
                Origin::MainWindow
            ),
            Err(LocalhostError::InvalidEnvironmentVariableName(name.into()))
        );
    }
}

#[test]
fn adding_a_service_to_a_missing_project_fails() {
    let id = Id::new();
    assert_eq!(
        Fixture::new().library().add_service(id, web("pnpm dev"), vec![], vec![], Origin::MainWindow),
        Err(LocalhostError::ProjectNotFound(id))
    );
}

// MARK: Persistence failures

#[test]
fn a_failed_write_leaves_the_library_unchanged() {
    let fixture = Fixture::new();
    let library = fixture.library();
    let project = library.add_project("App", key("app"), "/tmp/app", vec![], Origin::MainWindow).unwrap();
    let directory = fixture.projects_directory();
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o500)).unwrap();

    let result = library.edit_project(
        project.id,
        ProjectChanges { name: Some("Renamed".into()), ..Default::default() },
        Origin::MainWindow,
    );
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();

    // root ignores permissions, so the write can't fail when tests run as root (e.g. in a container).
    if unsafe_is_root() {
        return;
    }
    assert!(matches!(result, Err(LocalhostError::PersistenceFailed { .. })), "{result:?}");
    assert_eq!(library.project(project.id).unwrap().name, "App");
}

fn unsafe_is_root() -> bool {
    std::process::Command::new("id").arg("-u").output().is_ok_and(|output| output.stdout.starts_with(b"0\n"))
}

#[test]
fn a_file_from_a_newer_schema_version_is_reported_and_left_in_place() {
    let fixture = Fixture::new();
    let file = fixture.write_project_file(&format!("{}.json", Id::new()), r#"{"schemaVersion": 99, "project": {}}"#);
    let path = file.to_string_lossy().into_owned();

    let library = fixture.library();

    assert_eq!(
        library.unreadable_files(),
        [UnreadableProjectFile {
            path: path.clone(),
            moved_to: None,
            error: LocalhostError::UnsupportedSchemaVersion { version: 99, path },
        }]
    );
    assert!(library.projects().is_empty());
    assert!(file.exists());
}

#[test]
fn a_damaged_file_is_moved_aside_and_reported_and_the_other_projects_still_load() {
    for (contents, reason) in [
        ("not json", "It is damaged: it isn’t valid JSON."),
        (r#"{"schemaVersion": 1, "project": {"name": "A"}}"#, "It is damaged: “project.id” is missing."),
        (r#"{"schemaVersion": 1, "project": {"id": 7}}"#, "It is damaged: “project.id” has the wrong type."),
        ("", "It is damaged: it isn’t valid JSON."),
        (r#"{"project": {}}"#, "It is damaged: “schemaVersion” is missing."),
    ] {
        let fixture = Fixture::new();
        let good = fixture.library().add_project("Good", key("good"), "/tmp/good", vec![], Origin::MainWindow).unwrap();
        let damaged = fixture.write_project_file(&format!("{}.json", Id::new()), contents);
        let path = damaged.to_string_lossy().into_owned();

        let library = fixture.library();

        assert_eq!(library.projects().iter().map(|p| p.id).collect::<Vec<_>>(), [good.id]);
        assert_eq!(
            library.unreadable_files(),
            [UnreadableProjectFile {
                path: path.clone(),
                moved_to: Some(format!("{path}.unreadable")),
                error: LocalhostError::PersistenceFailed { path: path.clone(), reason: reason.into() },
            }],
            "{contents:?}"
        );
        assert!(!damaged.exists());
        assert_eq!(std::fs::read_to_string(format!("{path}.unreadable")).unwrap(), contents);
        // Reported once: the next launch doesn't see it again.
        assert!(fixture.library().unreadable_files().is_empty());
    }
}

#[test]
fn an_invalid_key_in_a_file_is_reported_with_its_field() {
    let fixture = Fixture::new();
    let file = fixture.write_project_file(
        &format!("{}.json", Id::new()),
        r#"{"schemaVersion":1,"project":{"id":"6F9A2C1E-0000-4000-8000-000000000009","name":"A","key":"Not A Key","path":"/tmp/a","sortOrder":0,"isFavorite":false,"services":[]}}"#,
    );
    let library = fixture.library();
    let path = file.to_string_lossy().into_owned();
    assert_eq!(
        library.unreadable_files()[0].error,
        LocalhostError::PersistenceFailed {
            path,
            reason: "It is damaged: “project.key”: Invalid key \"Not A Key\"".into()
        }
    );
}

#[test]
fn a_damaged_file_never_overwrites_one_moved_aside_before() {
    let fixture = Fixture::new();
    let name = format!("{}.json", Id::new());
    fixture.write_project_file(&name, "first");
    fixture.library();
    let second = fixture.write_project_file(&name, "second");
    let path = second.to_string_lossy().into_owned();

    let library = fixture.library();

    let moved: Vec<Option<String>> = library.unreadable_files().iter().map(|f| f.moved_to.clone()).collect();
    assert_eq!(moved, [Some(format!("{path}.unreadable-2"))]);
    assert_eq!(std::fs::read_to_string(format!("{path}.unreadable")).unwrap(), "first");
}

#[test]
fn a_second_copy_of_a_project_or_of_its_key_is_moved_aside_instead_of_loaded_twice() {
    let fixture = Fixture::new();
    let original = fixture.library().add_project("App", key("app"), "/tmp/app", vec![], Origin::MainWindow).unwrap();
    let json = std::fs::read_to_string(fixture.project_file(original.id)).unwrap();
    // A copy made in a file manager, and a different project that took the same key.
    let copy = fixture.write_project_file(&format!("{} copy.json", original.id), &json);
    let same_key = fixture.write_project_file(
        &format!("{}.json", Id::new()),
        &json.replace(&original.id.to_string(), &Id::new().to_string()),
    );

    let reloaded = fixture.library();

    assert_eq!(reloaded.projects().iter().map(|p| p.id).collect::<Vec<_>>(), [original.id]);
    let mut paths: Vec<String> = reloaded.unreadable_files().iter().map(|f| f.path.clone()).collect();
    paths.sort();
    let mut expected = vec![copy.to_string_lossy().into_owned(), same_key.to_string_lossy().into_owned()];
    expected.sort();
    assert_eq!(paths, expected);
    assert!(reloaded.unreadable_files().iter().any(|f| f.error == LocalhostError::DuplicateProjectKey(key("app"))));
    assert!(reloaded.unreadable_files().iter().all(|f| f.moved_to.is_some()));
}

// MARK: Events

#[test]
fn broadcasts_every_change_with_its_origin() {
    let library = Fixture::new().library();
    let mut events = library.events();

    let project = library.add_project("App", key("app"), "/tmp/app", vec![], Origin::MainWindow).unwrap();
    let service = library.add_service(project.id, web("pnpm dev"), vec![], vec![], Origin::MenuBar).unwrap();
    library.remove_project(project.id, Origin::MenuBar).unwrap();

    assert_eq!(
        events.try_recv(),
        Ok(ProjectLibraryEvent::ProjectAdded { project: project.clone(), origin: Origin::MainWindow })
    );
    match events.try_recv() {
        Ok(ProjectLibraryEvent::ProjectUpdated { project: updated, origin: Origin::MenuBar }) => {
            assert_eq!(updated.services, [service]);
        }
        other => panic!("expected projectUpdated from the menu bar, got {other:?}"),
    }
    assert_eq!(events.try_recv(), Ok(ProjectLibraryEvent::ProjectRemoved { id: project.id, origin: Origin::MenuBar }));
}

#[test]
fn subscribe_returns_the_current_projects_with_the_stream() {
    let library = Fixture::new().library();
    let existing = library.add_project("A", key("a"), "/tmp/a", vec![], Origin::MainWindow).unwrap();

    let (projects, mut events) = library.subscribe();
    let added = library.add_project("B", key("b"), "/tmp/b", vec![], Origin::MenuBar).unwrap();

    assert_eq!(projects, [existing]);
    assert_eq!(events.try_recv(), Ok(ProjectLibraryEvent::ProjectAdded { project: added, origin: Origin::MenuBar }));
}

#[test]
fn failed_changes_are_not_broadcast() {
    let library = Fixture::new().library();
    let mut events = library.events();

    let _ = library.add_project("", key("bad"), "/tmp/bad", vec![], Origin::MainWindow);
    let project = library.add_project("App", key("app"), "/tmp/app", vec![], Origin::MainWindow).unwrap();

    assert_eq!(events.try_recv(), Ok(ProjectLibraryEvent::ProjectAdded { project, origin: Origin::MainWindow }));
}

#[test]
fn a_dropped_subscriber_is_forgotten() {
    let library = Fixture::new().library();
    drop(library.events());
    library.add_project("App", key("app"), "/tmp/app", vec![], Origin::MainWindow).unwrap();
    let mut events = library.events();
    library.add_project("Two", key("two"), "/tmp/two", vec![], Origin::MainWindow).unwrap();
    assert!(matches!(events.try_recv(), Ok(ProjectLibraryEvent::ProjectAdded { .. })));
}
