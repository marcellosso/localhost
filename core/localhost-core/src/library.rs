use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::mpsc;

use crate::model::VariableValue;
use crate::secrets::SecretStore;
use crate::storage::{ProjectStorage, UnreadableProjectFile};
use crate::{
    EnvironmentProfile, Id, Key, LocalhostError, NewService, Origin, Project, Service, VariableInput, VariableLayer,
    Variables, validate,
};

/// A change to the Project Configuration, tagged with the Origin that caused it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectLibraryEvent {
    ProjectAdded {
        project: Project,
        origin: Origin,
    },
    /// The Project itself or one of its Services or Profiles was added, edited or removed.
    ProjectUpdated {
        project: Project,
        origin: Origin,
    },
    ProjectRemoved {
        id: Id,
        origin: Origin,
    },
}

/// What to do when an imported Project's Key is already used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyClash {
    /// Swap the imported Configuration into the Project with that Key,
    /// keeping its identity (and so its logs), sort order and favourite.
    Replace,
    /// Add the import under a free Key (`free_project_key`).
    KeepBoth,
}

/// The fields of a Project to change; `None` leaves a field as it is.
#[derive(Clone, Debug, Default)]
pub struct ProjectChanges {
    pub name: Option<String>,
    pub key: Option<Key>,
    pub path: Option<String>,
    pub is_favorite: Option<bool>,
    pub variables: Option<Vec<VariableInput>>,
}

/// The fields of a Service to change; `None` leaves a field as it is. For
/// `port` and `url`, `Some(None)` removes the value.
#[derive(Clone, Debug, Default)]
pub struct ServiceChanges {
    pub name: Option<String>,
    pub key: Option<Key>,
    pub working_directory: Option<String>,
    pub command_line: Option<String>,
    pub variables: Option<Vec<VariableInput>>,
    pub dependencies: Option<Vec<Id>>,
    pub is_excluded_from_start_all: Option<bool>,
    pub port: Option<Option<i64>>,
    pub url: Option<Option<String>>,
    pub port_can_change: Option<bool>,
}

/// The fields of an Environment Profile to change; `None` leaves a field as it is.
#[derive(Clone, Debug, Default)]
pub struct ProfileChanges {
    pub name: Option<String>,
    pub variables: Option<Vec<VariableInput>>,
}

/// Owns every Project and its Services: validates changes, persists them, and
/// broadcasts them as [`ProjectLibraryEvent`]s.
///
/// A change is persisted before it becomes visible; if writing fails, the
/// library is left unchanged and the error is returned. Changes are applied
/// one at a time, in call order.
///
/// Secret values go to the [`SecretStore`], never into the Project files:
/// clients learn a Secret's name and whether it is set, and read its value
/// only to reveal it.
pub struct ProjectLibrary {
    storage: ProjectStorage,
    secrets: Arc<dyn SecretStore>,
    state: Mutex<State>,
    unreadable_files: Vec<UnreadableProjectFile>,
}

struct State {
    projects: HashMap<Id, Project>,
    subscribers: Vec<mpsc::UnboundedSender<ProjectLibraryEvent>>,
}

impl ProjectLibrary {
    /// Loads every stored Project from `storage_root`. Files that can't be
    /// loaded are listed in `unreadable_files` rather than failing the rest;
    /// only an unreadable folder fails.
    pub fn open(storage_root: &Path, secrets: Arc<dyn SecretStore>) -> Result<ProjectLibrary, LocalhostError> {
        let storage = ProjectStorage::new(storage_root);
        let (projects, unreadable_files) = storage.load_all()?;
        Ok(ProjectLibrary {
            storage,
            secrets,
            state: Mutex::new(State {
                projects: projects.into_iter().map(|project| (project.id, project)).collect(),
                subscribers: Vec::new(),
            }),
            unreadable_files,
        })
    }

    /// The Project files that couldn't be loaded, to tell the user once.
    pub fn unreadable_files(&self) -> &[UnreadableProjectFile] {
        &self.unreadable_files
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    // MARK: Reading

    /// All Projects, in sort order.
    pub fn projects(&self) -> Vec<Project> {
        sorted(&self.state().projects)
    }

    pub fn project(&self, id: Id) -> Result<Project, LocalhostError> {
        self.state().projects.get(&id).cloned().ok_or(LocalhostError::ProjectNotFound(id))
    }

    /// A Service together with the Project it belongs to.
    pub fn service(&self, id: Id) -> Result<(Project, Service), LocalhostError> {
        let state = self.state();
        let (project, index) = locate_service(&state, id)?;
        Ok((project.clone(), project.services[index].clone()))
    }

    /// The value of a Secret, to reveal it. `None` if it has none stored.
    pub fn secret_value(&self, name: &str, layer: VariableLayer) -> Result<Option<String>, LocalhostError> {
        ensure_secret_exists(&self.state(), name, layer)?;
        self.secrets.value(&layer.secret_account(name))
    }

    /// Whether a Secret has a stored value.
    pub fn is_secret_set(&self, name: &str, layer: VariableLayer) -> Result<bool, LocalhostError> {
        Ok(self.secret_value(name, layer)?.is_some())
    }

    /// The variables of a Run of the Service: the Project's, then the
    /// Service's, then the active Environment Profile's, each overriding the
    /// one before, with Secrets resolved.
    #[allow(dead_code)] // Used by the ServiceManager, ported next.
    pub(crate) fn resolved_variables(&self, service_id: Id) -> Result<BTreeMap<String, String>, LocalhostError> {
        let (project, service) = self.service(service_id)?;
        let mut layers = vec![(VariableLayer::Project(project.id), project.variables.clone())];
        layers.push((VariableLayer::Service(service_id), service.variables()));
        if let Some(profile) = project.active_profile() {
            layers.push((VariableLayer::Profile(profile.id), profile.variables.clone()));
        }
        let mut resolved = BTreeMap::new();
        for (layer, variables) in layers {
            resolved.extend(variables.values);
            for name in variables.secret_names {
                let value = self.secrets.value(&layer.secret_account(&name))?;
                resolved.insert(name.clone(), value.ok_or(LocalhostError::SecretMissing(name))?);
            }
        }
        Ok(resolved)
    }

    /// All Projects plus a stream of every change after that snapshot. Taking
    /// both at once means no change falls between them.
    pub fn subscribe(&self) -> (Vec<Project>, mpsc::UnboundedReceiver<ProjectLibraryEvent>) {
        let mut state = self.state();
        let (sender, receiver) = mpsc::unbounded_channel();
        state.subscribers.push(sender);
        (sorted(&state.projects), receiver)
    }

    /// A stream of every change made after subscribing.
    pub fn events(&self) -> mpsc::UnboundedReceiver<ProjectLibraryEvent> {
        self.subscribe().1
    }

    // MARK: Projects

    /// Adds a Project, with `services` (e.g. reviewed Suggestions) in the
    /// same save: either all of it is added or none of it.
    pub fn add_project(
        &self,
        name: &str,
        key: Key,
        path: &str,
        services: Vec<NewService>,
        origin: Origin,
    ) -> Result<Project, LocalhostError> {
        let mut state = self.state();
        ensure_project_key_is_free(&state, &key)?;
        let mut project =
            Project::new(validate::name(name)?, key, validate::project_path(path)?, next_sort_order(&state));
        append(services, &mut project)?;
        self.commit(&mut state, &project)?;
        broadcast(&mut state, ProjectLibraryEvent::ProjectAdded { project: project.clone(), origin });
        Ok(project)
    }

    pub fn edit_project(&self, id: Id, changes: ProjectChanges, origin: Origin) -> Result<Project, LocalhostError> {
        let mut state = self.state();
        let mut project = state.projects.get(&id).cloned().ok_or(LocalhostError::ProjectNotFound(id))?;
        let mut stale_secrets = Vec::new();
        if let Some(name) = changes.name {
            project.name = validate::name(&name)?;
        }
        if let Some(key) = changes.key
            && key != project.key
        {
            ensure_project_key_is_free(&state, &key)?;
            project.key = key;
        }
        if let Some(path) = changes.path {
            project.path = validate::project_path(&path)?;
        }
        if let Some(is_favorite) = changes.is_favorite {
            project.is_favorite = is_favorite;
        }
        if let Some(variables) = changes.variables {
            (project.variables, stale_secrets) =
                self.apply(&variables, VariableLayer::Project(id), &project.variables)?;
        }
        self.commit(&mut state, &project)?;
        self.remove_secrets(&stale_secrets);
        broadcast(&mut state, ProjectLibraryEvent::ProjectUpdated { project: project.clone(), origin });
        Ok(project)
    }

    pub fn remove_project(&self, id: Id, origin: Origin) -> Result<(), LocalhostError> {
        let mut state = self.state();
        let project = state.projects.get(&id).cloned().ok_or(LocalhostError::ProjectNotFound(id))?;
        self.storage.delete(id)?;
        state.projects.remove(&id);
        self.remove_secrets(&all_secret_accounts(&project));
        broadcast(&mut state, ProjectLibraryEvent::ProjectRemoved { id, origin });
        Ok(())
    }

    /// `key` if no Project uses it, else the first free `key-2`, `key-3`, ….
    pub fn free_project_key(&self, key: &Key) -> Key {
        let state = self.state();
        let taken: HashSet<&str> = state.projects.values().map(|project| project.key.as_str()).collect();
        let base: String = key.as_str().chars().take(Key::MAX_LENGTH - 4).collect();
        let mut candidate = key.as_str().to_string();
        let mut suffix = 2;
        while taken.contains(candidate.as_str()) {
            candidate = format!("{base}-{suffix}");
            suffix += 1;
        }
        // Valid: a valid Key, cut short of the limit, plus `-<digits>`.
        Key::new(candidate).unwrap_or_else(|_| key.clone())
    }

    // MARK: Services

    pub fn add_service(
        &self,
        project_id: Id,
        service: NewService,
        variables: Vec<VariableInput>,
        dependencies: Vec<Id>,
        origin: Origin,
    ) -> Result<Service, LocalhostError> {
        let mut state = self.state();
        let mut project =
            state.projects.get(&project_id).cloned().ok_or(LocalhostError::ProjectNotFound(project_id))?;
        ensure_service_key_is_free(&project, &service.key)?;
        let mut service = make_service(service)?;
        let (variables, _) = self.apply(&variables, VariableLayer::Service(service.id), &Variables::default())?;
        service.set_variables(variables);
        let service_id = service.id;
        project.services.push(service);
        set_dependencies(&dependencies, service_id, &mut project)?;
        self.commit(&mut state, &project)?;
        broadcast(&mut state, ProjectLibraryEvent::ProjectUpdated { project: project.clone(), origin });
        Ok(project.services.last().cloned().expect("just added"))
    }

    /// Adds several Services to a Project in one save (e.g. reviewed
    /// Suggestions): either all of them are added or none.
    pub fn add_services(
        &self,
        services: Vec<NewService>,
        project_id: Id,
        origin: Origin,
    ) -> Result<Vec<Service>, LocalhostError> {
        let mut state = self.state();
        let mut project =
            state.projects.get(&project_id).cloned().ok_or(LocalhostError::ProjectNotFound(project_id))?;
        let existing = project.services.len();
        append(services, &mut project)?;
        self.commit(&mut state, &project)?;
        broadcast(&mut state, ProjectLibraryEvent::ProjectUpdated { project: project.clone(), origin });
        Ok(project.services[existing..].to_vec())
    }

    pub fn edit_service(&self, id: Id, changes: ServiceChanges, origin: Origin) -> Result<Service, LocalhostError> {
        let mut state = self.state();
        let (project, index) = locate_service(&state, id)?;
        let mut project = project.clone();
        let mut service = project.services[index].clone();
        let mut stale_secrets = Vec::new();
        if let Some(name) = changes.name {
            service.name = validate::name(&name)?;
        }
        if let Some(key) = changes.key
            && key != service.key
        {
            ensure_service_key_is_free(&project, &key)?;
            service.key = key;
        }
        if let Some(working_directory) = changes.working_directory {
            service.working_directory = validate::working_directory(&working_directory)?;
        }
        if let Some(command_line) = changes.command_line {
            service.command_line = validate::command_line(&command_line)?;
        }
        if let Some(variables) = changes.variables {
            let (variables, stale) = self.apply(&variables, VariableLayer::Service(id), &service.variables())?;
            service.set_variables(variables);
            stale_secrets = stale;
        }
        if let Some(is_excluded) = changes.is_excluded_from_start_all {
            service.is_excluded_from_start_all = is_excluded;
        }
        if let Some(port) = changes.port {
            service.port = validate::port(port)?;
        }
        if let Some(url) = changes.url {
            service.url = validate::url(url.as_deref())?;
        }
        if let Some(port_can_change) = changes.port_can_change {
            service.port_can_change = port_can_change;
        }
        project.services[index] = service;
        if let Some(dependencies) = changes.dependencies {
            set_dependencies(&dependencies, id, &mut project)?;
        }
        self.commit(&mut state, &project)?;
        self.remove_secrets(&stale_secrets);
        broadcast(&mut state, ProjectLibraryEvent::ProjectUpdated { project: project.clone(), origin });
        Ok(project.services[index].clone())
    }

    /// Removes the Service, and removes it from every other Service's Dependencies.
    pub fn remove_service(&self, id: Id, origin: Origin) -> Result<(), LocalhostError> {
        let mut state = self.state();
        let (project, index) = locate_service(&state, id)?;
        let mut project = project.clone();
        let removed = project.services.remove(index);
        for service in &mut project.services {
            service.dependencies.retain(|dependency| *dependency != id);
        }
        self.commit(&mut state, &project)?;
        self.remove_secrets(&secret_accounts(VariableLayer::Service(id), &removed.variables()));
        broadcast(&mut state, ProjectLibraryEvent::ProjectUpdated { project, origin });
        Ok(())
    }

    // MARK: Environment Profiles

    pub fn add_profile(
        &self,
        project_id: Id,
        name: &str,
        variables: Vec<VariableInput>,
        origin: Origin,
    ) -> Result<EnvironmentProfile, LocalhostError> {
        let mut state = self.state();
        let mut project =
            state.projects.get(&project_id).cloned().ok_or(LocalhostError::ProjectNotFound(project_id))?;
        let name = validate::name(name)?;
        ensure_profile_name_is_free(&project, &name)?;
        let id = Id::new();
        let (variables, _) = self.apply(&variables, VariableLayer::Profile(id), &Variables::default())?;
        let profile = EnvironmentProfile::new(id, name, variables);
        project.profiles.push(profile.clone());
        self.commit(&mut state, &project)?;
        broadcast(&mut state, ProjectLibraryEvent::ProjectUpdated { project, origin });
        Ok(profile)
    }

    pub fn edit_profile(
        &self,
        id: Id,
        changes: ProfileChanges,
        origin: Origin,
    ) -> Result<EnvironmentProfile, LocalhostError> {
        let mut state = self.state();
        let (project, index) = locate_profile(&state, id)?;
        let mut project = project.clone();
        let mut profile = project.profiles[index].clone();
        let mut stale_secrets = Vec::new();
        if let Some(name) = changes.name {
            let name = validate::name(&name)?;
            if name != profile.name {
                ensure_profile_name_is_free(&project, &name)?;
            }
            profile.name = name;
        }
        if let Some(variables) = changes.variables {
            (profile.variables, stale_secrets) =
                self.apply(&variables, VariableLayer::Profile(id), &profile.variables)?;
        }
        project.profiles[index] = profile.clone();
        self.commit(&mut state, &project)?;
        self.remove_secrets(&stale_secrets);
        broadcast(&mut state, ProjectLibraryEvent::ProjectUpdated { project, origin });
        Ok(profile)
    }

    /// Removes the Profile; if it was active, the Project has none active.
    pub fn remove_profile(&self, id: Id, origin: Origin) -> Result<(), LocalhostError> {
        let mut state = self.state();
        let (project, index) = locate_profile(&state, id)?;
        let mut project = project.clone();
        let removed = project.profiles.remove(index);
        if project.active_profile_id == Some(id) {
            project.active_profile_id = None;
        }
        self.commit(&mut state, &project)?;
        self.remove_secrets(&secret_accounts(VariableLayer::Profile(id), &removed.variables));
        broadcast(&mut state, ProjectLibraryEvent::ProjectUpdated { project, origin });
        Ok(())
    }

    /// Makes `profile_id` the Project's active Profile, or none with `None`.
    pub fn set_active_profile(
        &self,
        profile_id: Option<Id>,
        project_id: Id,
        origin: Origin,
    ) -> Result<(), LocalhostError> {
        let mut state = self.state();
        let mut project =
            state.projects.get(&project_id).cloned().ok_or(LocalhostError::ProjectNotFound(project_id))?;
        if let Some(profile_id) = profile_id
            && project.profile(profile_id).is_none()
        {
            return Err(LocalhostError::ProfileNotFound(profile_id));
        }
        if project.active_profile_id == profile_id {
            return Ok(());
        }
        project.active_profile_id = profile_id;
        self.commit(&mut state, &project)?;
        broadcast(&mut state, ProjectLibraryEvent::ProjectUpdated { project, origin });
        Ok(())
    }

    // MARK: Private

    /// The layer's new variables from `inputs`, with Secret values written to
    /// the store, plus the accounts of Secrets that no longer exist, to remove
    /// once the change is committed.
    fn apply(
        &self,
        inputs: &[VariableInput],
        layer: VariableLayer,
        current: &Variables,
    ) -> Result<(Variables, Vec<String>), LocalhostError> {
        let mut values = BTreeMap::new();
        let mut secret_values: Vec<(String, String)> = Vec::new();
        let mut names = HashSet::new();
        for input in inputs {
            if !validate::is_valid_variable_name(&input.name) {
                return Err(LocalhostError::InvalidEnvironmentVariableName(input.name.clone()));
            }
            if !names.insert(input.name.as_str()) {
                return Err(LocalhostError::DuplicateVariableName(input.name.clone()));
            }
            match &input.value {
                VariableValue::Plain(value) => {
                    values.insert(input.name.clone(), value.clone());
                }
                VariableValue::Secret(value) => secret_values.push((input.name.clone(), value.clone())),
                VariableValue::ExistingSecret { previous_name } => {
                    if !current.secret_names.contains(previous_name) {
                        return Err(LocalhostError::SecretMissing(previous_name.clone()));
                    }
                    // Read every kept value before writing any: a new Secret
                    // may take the name a renamed one had.
                    if *previous_name != input.name {
                        let value = self
                            .secrets
                            .value(&layer.secret_account(previous_name))?
                            .ok_or_else(|| LocalhostError::SecretMissing(previous_name.clone()))?;
                        secret_values.push((input.name.clone(), value));
                    }
                }
            }
        }
        for (name, value) in &secret_values {
            self.secrets.set_value(value, &layer.secret_account(name))?;
        }
        let mut secret_names: Vec<String> = inputs
            .iter()
            .filter(|input| !matches!(input.value, VariableValue::Plain(_)))
            .map(|input| input.name.clone())
            .collect();
        let stale = current
            .secret_names
            .iter()
            .filter(|name| !secret_names.contains(name))
            .map(|name| layer.secret_account(name))
            .collect();
        secret_names.sort();
        Ok((Variables { values, secret_names }, stale))
    }

    fn commit(&self, state: &mut State, project: &Project) -> Result<(), LocalhostError> {
        self.storage.save(project)?;
        state.projects.insert(project.id, project.clone());
        Ok(())
    }

    /// Best effort: a leftover item is harmless, the configuration no longer names it.
    fn remove_secrets(&self, accounts: &[String]) {
        for account in accounts {
            let _ = self.secrets.remove_value(account);
        }
    }
}

fn sorted(projects: &HashMap<Id, Project>) -> Vec<Project> {
    let mut projects: Vec<Project> = projects.values().cloned().collect();
    projects.sort_by(|a, b| (a.sort_order, &a.name).cmp(&(b.sort_order, &b.name)));
    projects
}

fn broadcast(state: &mut State, event: ProjectLibraryEvent) {
    state.subscribers.retain(|subscriber| subscriber.send(event.clone()).is_ok());
}

fn next_sort_order(state: &State) -> i64 {
    state.projects.values().map(|project| project.sort_order).max().map_or(0, |max| max + 1)
}

fn make_service(new: NewService) -> Result<Service, LocalhostError> {
    Ok(Service {
        id: Id::new(),
        name: validate::name(&new.name)?,
        key: new.key,
        working_directory: validate::working_directory(&new.working_directory)?,
        command_line: validate::command_line(&new.command_line)?,
        environment: BTreeMap::new(),
        secret_names: Vec::new(),
        dependencies: Vec::new(),
        is_excluded_from_start_all: new.is_excluded_from_start_all,
        port: validate::port(new.port)?,
        url: validate::url(new.url.as_deref())?,
        port_can_change: new.port_can_change,
    })
}

fn append(services: Vec<NewService>, project: &mut Project) -> Result<(), LocalhostError> {
    for new in services {
        ensure_service_key_is_free(project, &new.key)?;
        project.services.push(make_service(new)?);
    }
    Ok(())
}

/// Replaces the Dependencies of `service_id`, rejecting Services of other
/// Projects and cycles. Duplicates are dropped; the order is kept.
fn set_dependencies(dependencies: &[Id], service_id: Id, project: &mut Project) -> Result<(), LocalhostError> {
    let index = project
        .services
        .iter()
        .position(|service| service.id == service_id)
        .ok_or(LocalhostError::ServiceNotFound(service_id))?;
    project.services[index].dependencies = Vec::new();
    let mut accepted: Vec<Id> = Vec::new();
    for &dependency in dependencies {
        if accepted.contains(&dependency) {
            continue;
        }
        if project.service(dependency).is_none() {
            return Err(LocalhostError::ServiceNotFound(dependency));
        }
        if let Some(cycle) = project.dependency_cycle(service_id, dependency) {
            return Err(LocalhostError::DependencyCycle(cycle));
        }
        accepted.push(dependency);
        project.services[index].dependencies = accepted.clone();
    }
    Ok(())
}

fn secret_accounts(layer: VariableLayer, variables: &Variables) -> Vec<String> {
    variables.secret_names.iter().map(|name| layer.secret_account(name)).collect()
}

/// The accounts of every Secret of the Project, on every layer.
fn all_secret_accounts(project: &Project) -> Vec<String> {
    let mut accounts = secret_accounts(VariableLayer::Project(project.id), &project.variables);
    for service in &project.services {
        accounts.extend(secret_accounts(VariableLayer::Service(service.id), &service.variables()));
    }
    for profile in &project.profiles {
        accounts.extend(secret_accounts(VariableLayer::Profile(profile.id), &profile.variables));
    }
    accounts
}

fn ensure_secret_exists(state: &State, name: &str, layer: VariableLayer) -> Result<(), LocalhostError> {
    let variables = match layer {
        VariableLayer::Project(id) => {
            state.projects.get(&id).ok_or(LocalhostError::ProjectNotFound(id))?.variables.clone()
        }
        VariableLayer::Service(id) => {
            let (project, index) = locate_service(state, id)?;
            project.services[index].variables()
        }
        VariableLayer::Profile(id) => {
            let (project, index) = locate_profile(state, id)?;
            project.profiles[index].variables.clone()
        }
    };
    if !variables.secret_names.iter().any(|secret| secret == name) {
        return Err(LocalhostError::SecretMissing(name.to_string()));
    }
    Ok(())
}

fn locate_service(state: &State, id: Id) -> Result<(&Project, usize), LocalhostError> {
    state
        .projects
        .values()
        .find_map(|project| project.services.iter().position(|service| service.id == id).map(|index| (project, index)))
        .ok_or(LocalhostError::ServiceNotFound(id))
}

fn locate_profile(state: &State, id: Id) -> Result<(&Project, usize), LocalhostError> {
    state
        .projects
        .values()
        .find_map(|project| project.profiles.iter().position(|profile| profile.id == id).map(|index| (project, index)))
        .ok_or(LocalhostError::ProfileNotFound(id))
}

fn ensure_project_key_is_free(state: &State, key: &Key) -> Result<(), LocalhostError> {
    if state.projects.values().any(|project| project.key == *key) {
        return Err(LocalhostError::DuplicateProjectKey(key.clone()));
    }
    Ok(())
}

fn ensure_service_key_is_free(project: &Project, key: &Key) -> Result<(), LocalhostError> {
    if project.services.iter().any(|service| service.key == *key) {
        return Err(LocalhostError::DuplicateServiceKey { key: key.clone(), project_id: project.id });
    }
    Ok(())
}

fn ensure_profile_name_is_free(project: &Project, name: &str) -> Result<(), LocalhostError> {
    if project.profiles.iter().any(|profile| profile.name == name) {
        return Err(LocalhostError::DuplicateProfileName(name.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemorySecretStore;

    fn plain(pairs: &[(&str, &str)]) -> Vec<VariableInput> {
        pairs.iter().map(|(name, value)| VariableInput::plain(*name, *value)).collect()
    }

    #[test]
    fn a_run_gets_project_then_service_then_active_profile_variables_with_secrets_resolved() {
        let root = tempfile::tempdir().unwrap();
        let secrets = Arc::new(MemorySecretStore::new());
        let library = ProjectLibrary::open(root.path(), secrets.clone()).unwrap();
        let project =
            library.add_project("App", Key::new("app").unwrap(), "/tmp/app", vec![], Origin::MainWindow).unwrap();
        let mut project_variables = plain(&[("A", "project"), ("B", "project"), ("C", "project")]);
        project_variables.push(VariableInput::secret("TOKEN", "project-secret"));
        let changes = ProjectChanges { variables: Some(project_variables), ..Default::default() };
        library.edit_project(project.id, changes, Origin::MainWindow).unwrap();
        let profile =
            library.add_profile(project.id, "Testing", plain(&[("C", "profile")]), Origin::MainWindow).unwrap();
        let service = library
            .add_service(
                project.id,
                NewService::new("Web", Key::new("web").unwrap(), "true"),
                plain(&[("B", "service"), ("C", "service")]),
                vec![],
                Origin::MainWindow,
            )
            .unwrap();

        let expected = |c: &str| -> BTreeMap<String, String> {
            [("A", "project"), ("B", "service"), ("C", c), ("TOKEN", "project-secret")]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        assert_eq!(library.resolved_variables(service.id), Ok(expected("service")), "an inactive Profile adds nothing");
        library.set_active_profile(Some(profile.id), project.id, Origin::MainWindow).unwrap();
        assert_eq!(library.resolved_variables(service.id), Ok(expected("profile")));

        secrets.remove_value(&VariableLayer::Project(project.id).secret_account("TOKEN")).unwrap();
        assert_eq!(library.resolved_variables(service.id), Err(LocalhostError::SecretMissing("TOKEN".into())));
    }
}
