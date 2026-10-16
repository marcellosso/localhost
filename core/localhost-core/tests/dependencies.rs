//! Ported from `DependencyTests.swift`: the Dependency graph and how the
//! library validates and maintains it.

mod common;

use common::{Fixture, key, some_project_path};
use localhost_core::{Id, LocalhostError, NewService, Origin, Project, ProjectLibrary, Service, ServiceChanges};

struct Setup {
    fixture: Fixture,
    library: ProjectLibrary,
    project: Project,
}

impl Setup {
    fn new() -> Setup {
        let fixture = Fixture::new();
        let library = fixture.library();
        let project =
            library.add_project("App", key("app"), &some_project_path("app"), vec![], Origin::MainWindow).unwrap();
        Setup { fixture, library, project }
    }

    fn service(&self, raw: &str, dependencies: &[&Service]) -> Service {
        self.library
            .add_service(
                self.project.id,
                NewService::new(raw, key(raw), "true"),
                vec![],
                dependencies.iter().map(|s| s.id).collect(),
                Origin::MainWindow,
            )
            .unwrap()
    }

    fn current(&self) -> Project {
        self.library.project(self.project.id).unwrap()
    }

    fn set_dependencies(&self, service: &Service, dependencies: &[&Service]) -> Result<Service, LocalhostError> {
        let changes =
            ServiceChanges { dependencies: Some(dependencies.iter().map(|s| s.id).collect()), ..Default::default() };
        self.library.edit_service(service.id, changes, Origin::MainWindow)
    }
}

// MARK: Editing

#[test]
fn stores_dependencies_in_order_without_duplicates_and_reloads_them() {
    let setup = Setup::new();
    let database = setup.service("database", &[]);
    let cache = setup.service("cache", &[]);
    let mut api = NewService::new("API", key("api"), "true");
    api.is_excluded_from_start_all = true;
    let api = setup
        .library
        .add_service(setup.project.id, api, vec![], vec![database.id, cache.id, database.id], Origin::MainWindow)
        .unwrap();

    assert_eq!(api.dependencies, [database.id, cache.id]);
    assert!(api.is_excluded_from_start_all);
    assert_eq!(setup.fixture.library().service(api.id).unwrap().1, api);
}

#[test]
fn rejects_a_dependency_on_itself() {
    let setup = Setup::new();
    let api = setup.service("api", &[]);
    assert_eq!(
        setup.set_dependencies(&api, &[&api]),
        Err(LocalhostError::DependencyCycle(vec![key("api"), key("api")]))
    );
}

#[test]
fn rejects_a_dependency_that_closes_a_cycle_naming_it() {
    let setup = Setup::new();
    let database = setup.service("database", &[]);
    let worker = setup.service("worker", &[&database]);
    let api = setup.service("api", &[&worker]);

    assert_eq!(
        setup.set_dependencies(&database, &[&api]),
        Err(LocalhostError::DependencyCycle(vec![key("database"), key("api"), key("worker"), key("database")]))
    );
    assert_eq!(setup.current().service(database.id).unwrap().dependencies, Vec::<Id>::new());
}

#[test]
fn rejects_a_dependency_on_a_service_of_another_project() {
    let setup = Setup::new();
    let api = setup.service("api", &[]);
    let other = setup
        .library
        .add_project("Other", key("other"), &some_project_path("other"), vec![], Origin::MainWindow)
        .unwrap();
    let foreign = setup
        .library
        .add_service(other.id, NewService::new("DB", key("db"), "true"), vec![], vec![], Origin::MainWindow)
        .unwrap();
    assert_eq!(setup.set_dependencies(&api, &[&foreign]), Err(LocalhostError::ServiceNotFound(foreign.id)));
}

#[test]
fn removing_a_service_removes_it_from_every_dependency_list() {
    let setup = Setup::new();
    let database = setup.service("database", &[]);
    let cache = setup.service("cache", &[]);
    let api = setup.service("api", &[&database, &cache]);

    setup.library.remove_service(database.id, Origin::MainWindow).unwrap();

    assert_eq!(setup.current().service(api.id).unwrap().dependencies, [cache.id]);
}

// MARK: Graph

#[test]
fn allows_a_dependency_that_closes_no_cycle() {
    let setup = Setup::new();
    let database = setup.service("database", &[]);
    let api = setup.service("api", &[&database]);
    let web = setup.service("web", &[&api]);
    let project = setup.current();

    assert_eq!(project.dependency_cycle(web.id, database.id), None);
    assert_eq!(
        project.dependency_cycle(database.id, web.id),
        Some(vec![key("database"), key("web"), key("api"), key("database")])
    );
}

#[test]
fn orders_services_after_their_dependencies_otherwise_in_project_order() {
    let setup = Setup::new();
    let web = setup.service("web", &[]);
    let api = setup.service("api", &[]);
    let database = setup.service("database", &[]);
    let docs = setup.service("docs", &[]);
    setup.set_dependencies(&web, &[&api]).unwrap();
    setup.set_dependencies(&api, &[&database]).unwrap();
    let project = setup.current();

    assert_eq!(
        project.dependency_order(&[web.id, api.id, database.id, docs.id]),
        [database.id, api.id, web.id, docs.id]
    );
    assert_eq!(project.dependency_order(&[web.id, docs.id]), [web.id, docs.id]);
    assert_eq!(project.dependents(database.id).iter().map(|s| s.id).collect::<Vec<_>>(), [api.id]);
}
