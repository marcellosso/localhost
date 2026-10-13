//! The library half of `EnvironmentTests.swift`: Project variables,
//! Environment Profiles and Secrets. The Run half (what a started Service
//! sees, "Restart to apply") moves over with the ServiceManager.

mod common;

use common::{Fixture, key};
use localhost_core::{
    LocalhostError, NewService, Origin, ProfileChanges, Project, ProjectChanges, ProjectLibrary, Service,
    ServiceChanges, VariableInput, VariableLayer, Variables,
};

struct Setup {
    fixture: Fixture,
    library: ProjectLibrary,
    project: Project,
}

impl Setup {
    fn new() -> Setup {
        let fixture = Fixture::new();
        let library = fixture.library();
        let project = library.add_project("App", key("app"), "/tmp/app", vec![], Origin::MainWindow).unwrap();
        Setup { fixture, library, project }
    }

    fn service(&self, variables: Vec<VariableInput>) -> Service {
        self.library
            .add_service(
                self.project.id,
                NewService::new("Web", key("web"), "true"),
                variables,
                vec![],
                Origin::MainWindow,
            )
            .unwrap()
    }

    fn edit_variables(&self, service: &Service, variables: Vec<VariableInput>) -> Result<Service, LocalhostError> {
        self.library.edit_service(
            service.id,
            ServiceChanges { variables: Some(variables), ..Default::default() },
            Origin::MainWindow,
        )
    }
}

// MARK: Profiles

#[test]
fn profiles_have_unique_names_and_removing_the_active_one_leaves_none_active() {
    let setup = Setup::new();
    let library = &setup.library;
    let development = library.add_profile(setup.project.id, "Development", vec![], Origin::MainWindow).unwrap();
    let testing = library.add_profile(setup.project.id, "Testing", vec![], Origin::MainWindow).unwrap();
    assert_eq!(
        library.edit_profile(
            testing.id,
            ProfileChanges { name: Some("Development".into()), ..Default::default() },
            Origin::MainWindow
        ),
        Err(LocalhostError::DuplicateProfileName("Development".into()))
    );
    assert_eq!(
        library.set_active_profile(Some(setup.project.id), setup.project.id, Origin::MainWindow),
        Err(LocalhostError::ProfileNotFound(setup.project.id))
    );

    library.set_active_profile(Some(development.id), setup.project.id, Origin::MainWindow).unwrap();
    assert_eq!(library.project(setup.project.id).unwrap().active_profile().unwrap().name, "Development");
    library.remove_profile(development.id, Origin::MainWindow).unwrap();

    let reloaded = setup.fixture.library().project(setup.project.id).unwrap();
    assert_eq!(reloaded.active_profile_id, None);
    assert_eq!(reloaded.profiles.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Testing"]);
}

#[test]
fn a_project_saved_before_profiles_existed_loads_with_none() {
    let fixture = Fixture::new();
    fixture.write_project_file(
        "6F9A2C1E-0000-4000-8000-000000000009.json",
        r#"{"schemaVersion":1,"project":{"id":"6F9A2C1E-0000-4000-8000-000000000009","name":"Old","key":"old","path":"/tmp/old","sortOrder":0,"isFavorite":false,"services":[{"id":"6F9A2C1E-0000-4000-8000-00000000000A","name":"Web","key":"web","workingDirectory":".","commandLine":"pnpm dev","environment":{"A":"1"}}]}}"#,
    );

    let project = fixture.library().projects().pop().unwrap();
    assert_eq!(project.variables, Variables::default());
    assert!(project.profiles.is_empty());
    assert_eq!(project.active_profile_id, None);
    let variables = project.services[0].variables();
    assert_eq!(variables.values.get("A").map(String::as_str), Some("1"));
    assert!(variables.secret_names.is_empty());
}

// MARK: Secrets

#[test]
fn a_secrets_value_is_stored_apart_from_the_project_file() {
    let setup = Setup::new();
    let service =
        setup.service(vec![VariableInput::secret("TOKEN", "s3cr3t-value"), VariableInput::plain("DEBUG", "1")]);
    let layer = VariableLayer::Service(service.id);

    let json = std::fs::read_to_string(setup.fixture.project_file(setup.project.id)).unwrap();
    assert!(json.contains("TOKEN"));
    assert!(!json.contains("s3cr3t-value"));
    assert_eq!(service.secret_names, ["TOKEN"]);
    assert_eq!(service.environment.len(), 1);
    assert_eq!(setup.library.is_secret_set("TOKEN", layer), Ok(true));
    assert_eq!(setup.library.secret_value("TOKEN", layer), Ok(Some("s3cr3t-value".into())));
    assert_eq!(setup.library.secret_value("DEBUG", layer), Err(LocalhostError::SecretMissing("DEBUG".into())));
}

#[test]
fn a_secret_keeps_its_value_when_kept_or_renamed_and_loses_it_when_removed_or_made_plain() {
    let setup = Setup::new();
    let service = setup.service(vec![VariableInput::secret("TOKEN", "one"), VariableInput::secret("OTHER", "two")]);
    let layer = VariableLayer::Service(service.id);

    setup
        .edit_variables(
            &service,
            vec![VariableInput::renamed_secret("API_TOKEN", "TOKEN"), VariableInput::existing_secret("OTHER")],
        )
        .unwrap();
    assert_eq!(setup.library.secret_value("API_TOKEN", layer), Ok(Some("one".into())));
    assert_eq!(setup.library.secret_value("OTHER", layer), Ok(Some("two".into())));
    assert_eq!(setup.fixture.secrets.accounts(), [layer.secret_account("API_TOKEN"), layer.secret_account("OTHER")]);

    let plain = setup.edit_variables(&service, vec![VariableInput::plain("API_TOKEN", "visible")]).unwrap();
    assert!(setup.fixture.secrets.accounts().is_empty());
    assert_eq!(plain.environment.get("API_TOKEN").map(String::as_str), Some("visible"));
}

#[test]
fn a_new_secret_may_take_the_name_a_renamed_one_had() {
    let setup = Setup::new();
    let service = setup.service(vec![VariableInput::secret("TOKEN", "old")]);
    let layer = VariableLayer::Service(service.id);

    setup
        .edit_variables(
            &service,
            vec![VariableInput::renamed_secret("OLD_TOKEN", "TOKEN"), VariableInput::secret("TOKEN", "new")],
        )
        .unwrap();

    assert_eq!(setup.library.secret_value("OLD_TOKEN", layer), Ok(Some("old".into())));
    assert_eq!(setup.library.secret_value("TOKEN", layer), Ok(Some("new".into())));
}

#[test]
fn removing_a_service_profile_or_project_removes_its_secrets() {
    let setup = Setup::new();
    let service = setup.service(vec![VariableInput::secret("S", "1")]);
    let profile = setup
        .library
        .add_profile(setup.project.id, "P", vec![VariableInput::secret("S", "2")], Origin::MainWindow)
        .unwrap();
    setup
        .library
        .edit_project(
            setup.project.id,
            ProjectChanges { variables: Some(vec![VariableInput::secret("S", "3")]), ..Default::default() },
            Origin::MainWindow,
        )
        .unwrap();
    assert_eq!(setup.fixture.secrets.accounts().len(), 3);

    setup.library.remove_service(service.id, Origin::MainWindow).unwrap();
    setup.library.remove_profile(profile.id, Origin::MainWindow).unwrap();
    assert_eq!(setup.fixture.secrets.accounts(), [VariableLayer::Project(setup.project.id).secret_account("S")]);
    setup.library.remove_project(setup.project.id, Origin::MainWindow).unwrap();
    assert!(setup.fixture.secrets.accounts().is_empty());
}

#[test]
fn keeping_a_secret_that_does_not_exist_fails() {
    let setup = Setup::new();
    let service = setup.service(vec![]);
    assert_eq!(
        setup.edit_variables(&service, vec![VariableInput::existing_secret("NOPE")]),
        Err(LocalhostError::SecretMissing("NOPE".into()))
    );
}

#[test]
fn variable_names_are_validated_on_every_layer() {
    let setup = Setup::new();
    assert_eq!(
        setup.library.edit_project(
            setup.project.id,
            ProjectChanges { variables: Some(vec![VariableInput::secret("MY-VAR", "x")]), ..Default::default() },
            Origin::MainWindow,
        ),
        Err(LocalhostError::InvalidEnvironmentVariableName("MY-VAR".into()))
    );
    assert_eq!(
        setup.library.add_profile(
            setup.project.id,
            "P",
            vec![VariableInput::plain("A", "1"), VariableInput::secret("A", "2")],
            Origin::MainWindow
        ),
        Err(LocalhostError::DuplicateVariableName("A".into()))
    );
    assert!(setup.fixture.secrets.accounts().is_empty());
}
