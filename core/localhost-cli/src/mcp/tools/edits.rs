//! The tools that change the Project Configuration: adding, editing and
//! removing Projects and Services. Left out by `localhost mcp --read-only`.
//!
//! Every change goes through the `ProjectLibrary`, so it is validated as
//! the app's editors validate it and saved where the app reads it. Then the
//! shared core is told, and an open app reads the Configuration again: what
//! the agent changes shows there without a restart. Commands are stored as
//! given, never run here; a Project's folder is never written to.
//!
//! Secrets stay the app's to set: an agent sets plain variables only, and
//! every Secret a layer has is kept as it is.

use localhost_core::{
    Id, Key, NewService, Origin, Project, ProjectChanges, Service, ServiceChanges, VariableInput, Variables,
    existing_project_folder, suggested_project_key,
};
use serde_json::{Map, Value, json};

use super::{ToolError, Tools, optional_string, required_string, summary};
use crate::resolve;

/// The names of the tools this module answers, which read-only mode leaves out.
pub const NAMES: [&str; 6] =
    ["create_project", "update_project", "delete_project", "create_service", "update_service", "delete_service"];

pub fn definitions() -> Vec<Value> {
    let project =
        json!({ "type": "string", "description": "The Project's Key (as list_projects shows it), or its name." });
    let service =
        json!({ "type": "string", "description": "The Service's Key (as list_services shows it), or its name." });
    let env = json!({
        "type": "object",
        "additionalProperties": { "type": ["string", "number", "boolean"] },
        "description": "Plain environment variables by name, e.g. {\"NODE_ENV\": \"development\"}. Replaces the plain variables there are; Secrets are kept, and only the localhost app sets them.",
    });
    let confirm = json!({ "type": "boolean", "const": true, "description": "Must be true: ask the user before removing anything." });
    let edits =
        json!({ "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false });
    let removes =
        json!({ "readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": false });
    let service_fields = |required: bool| {
        let mut fields = json!({
            "name": { "type": "string", "description": "What the developer calls it, e.g. \"Web\" or \"Postgres\"." },
            "key": { "type": "string", "description": "A short slug unique in the Project: lowercase letters, digits and single hyphens, e.g. web. Leave out to make one from the name." },
            "command_line": { "type": "string", "description": "The command that runs it, as typed in a terminal, e.g. \"pnpm dev\". Run by the user's login shell (cmd.exe on Windows) with PORT set to the Service's Port. Stored as given, not run now." },
            "working_directory": { "type": "string", "description": "Where the command runs, relative to the Project's folder, e.g. \"apps/web\". \".\" (the default) is the folder itself." },
            "port": { "type": ["integer", "null"], "minimum": 1, "maximum": 65535, "description": "The TCP port it listens on: it is running once the port accepts connections, and its command gets it as $PORT. Leave out for none." },
            "url": { "type": ["string", "null"], "description": "Where to open it, e.g. \"http://localhost:3000/admin\". Leave out to use http://localhost:<port>." },
            "port_can_change": { "type": "boolean", "description": "Whether a Run may move to the next free port when the Port is taken. Turn off for commands that ignore $PORT. Default true." },
            "in_start_all": { "type": "boolean", "description": "Whether start_project starts it. Default true." },
            "dependencies": { "type": "array", "items": { "type": "string" }, "description": "Keys of Services of the same Project that must be running (or have exited) before it starts." },
            "env": env,
        });
        if !required {
            fields["name"]["description"] = "A new name.".into();
            fields["key"]["description"] = "A new Key: lowercase letters, digits and single hyphens.".into();
            fields["port"]["description"] = "A new Port, or null for none.".into();
            fields["url"]["description"] = "A new URL, or null for none.".into();
            fields["dependencies"]["description"] =
                "Replaces its Dependencies: Keys of Services of the same Project; [] for none.".into();
        }
        fields
    };

    let mut create_service = service_fields(true);
    create_service["project"] = project.clone();
    let mut update_service = service_fields(false);
    update_service["project"] = project.clone();
    update_service["service"] = service.clone();
    vec![
        json!({
            "name": "create_project",
            "title": "Add a Project",
            "description": "Adds a Project: a folder on this machine whose Services localhost runs. Add its Services next with create_service. Nothing is written into the folder. Returns the Project, with its Key and id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "What the developer calls it, e.g. \"My SaaS\"." },
                    "path": { "type": "string", "description": "Absolute path of the Project's folder on this machine. It must exist." },
                    "key": { "type": "string", "description": "A short slug unique among Projects: lowercase letters, digits and single hyphens, e.g. my-saas. Leave out to make one from the name." },
                    "favorite": { "type": "boolean", "description": "Whether it is a favourite, always in the menu bar. Default false." },
                    "env": env,
                },
                "required": ["name", "path"],
                "additionalProperties": false,
            },
            "annotations": edits,
        }),
        json!({
            "name": "update_project",
            "title": "Edit a Project",
            "description": "Changes the given fields of a Project; the others stay. Returns the Project as saved. A running Service keeps its configuration until it is restarted.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "name": { "type": "string", "description": "A new name." },
                    "key": { "type": "string", "description": "A new Key: lowercase letters, digits and single hyphens." },
                    "path": { "type": "string", "description": "A new absolute folder path. It must exist." },
                    "favorite": { "type": "boolean" },
                    "env": env,
                },
                "required": ["project"],
                "additionalProperties": false,
            },
            "annotations": edits,
        }),
        json!({
            "name": "delete_project",
            "title": "Remove a Project",
            "description": "Removes a Project and its Services from localhost: stops those running, then deletes their configuration and logs. Its folder and files are never touched. Ask the user first, then pass confirm: true. Returns what was removed.",
            "inputSchema": {
                "type": "object",
                "properties": { "project": project, "confirm": confirm },
                "required": ["project", "confirm"],
                "additionalProperties": false,
            },
            "annotations": removes,
        }),
        json!({
            "name": "create_service",
            "title": "Add a Service",
            "description": "Adds a Service to a Project: a long-running command such as a dev server, a database or a worker. The command is stored, not run: start it with start_service. Returns the Service, with its Key and id.",
            "inputSchema": {
                "type": "object",
                "properties": create_service,
                "required": ["project", "name", "command_line"],
                "additionalProperties": false,
            },
            "annotations": edits,
        }),
        json!({
            "name": "update_service",
            "title": "Edit a Service",
            "description": "Changes the given fields of a Service; the others stay. Returns the Service as saved. If it is running, the change applies when it is restarted.",
            "inputSchema": {
                "type": "object",
                "properties": update_service,
                "required": ["project", "service"],
                "additionalProperties": false,
            },
            "annotations": edits,
        }),
        json!({
            "name": "delete_service",
            "title": "Remove a Service",
            "description": "Removes a Service from its Project: stops it if it is running, then deletes its configuration and logs, and drops it from other Services' Dependencies. Ask the user first, then pass confirm: true. Returns what was removed.",
            "inputSchema": {
                "type": "object",
                "properties": { "project": project, "service": service, "confirm": confirm },
                "required": ["project", "service", "confirm"],
                "additionalProperties": false,
            },
            "annotations": removes,
        }),
    ]
}

impl Tools {
    /// Answers one of `NAMES`.
    pub(super) async fn edit(&self, name: &str, arguments: &Map<String, Value>) -> Result<Value, ToolError> {
        let result = match name {
            "create_project" => self.create_project(arguments),
            "update_project" => self.update_project(arguments),
            "delete_project" => self.delete_project(arguments).await,
            "create_service" => self.create_service(arguments),
            "update_service" => self.update_service(arguments),
            "delete_service" => self.delete_service(arguments).await,
            _ => unreachable!("only the names in `NAMES`"),
        }?;
        // An app that misses this reads the change at its next launch.
        let _ = self.targets.announce_projects_saved(Origin::Agent).await;
        Ok(result)
    }

    // MARK: Projects

    fn create_project(&self, arguments: &Map<String, Value>) -> Result<Value, ToolError> {
        let name = required_string(arguments, "name")?;
        let path = existing_project_folder(required_string(arguments, "path")?)?;
        let key = match optional_string(arguments, "key")? {
            Some(key) => Key::new(key)?,
            None => suggested_project_key(&self.library.projects(), name, None),
        };
        let project = self.library.add_project(name, key, &path, vec![], Origin::Agent)?;
        let changes = ProjectChanges {
            is_favorite: optional_bool(arguments, "favorite")?,
            variables: variables(arguments, &Variables::default())?,
            ..Default::default()
        };
        let project = match changes.is_favorite.is_some() || changes.variables.is_some() {
            true => self.library.edit_project(project.id, changes, Origin::Agent).inspect_err(|_| {
                let _ = self.library.remove_project(project.id, Origin::Agent);
            })?,
            false => project,
        };
        Ok(json!({ "project": project_value(&project) }))
    }

    fn update_project(&self, arguments: &Map<String, Value>) -> Result<Value, ToolError> {
        let project = self.project(arguments)?;
        let path = match optional_string(arguments, "path")? {
            Some(path) if project.execution_target.is_this_machine() => Some(existing_project_folder(path)?),
            path => path.map(str::to_string),
        };
        let changes = ProjectChanges {
            name: optional_string(arguments, "name")?.map(str::to_string),
            key: optional_string(arguments, "key")?.map(Key::new).transpose()?,
            path,
            execution_target: None,
            is_favorite: optional_bool(arguments, "favorite")?,
            variables: variables(arguments, &project.variables)?,
        };
        let project = self.library.edit_project(project.id, changes, Origin::Agent)?;
        Ok(json!({ "project": project_value(&project) }))
    }

    async fn delete_project(&self, arguments: &Map<String, Value>) -> Result<Value, ToolError> {
        let project = self.project(arguments)?;
        confirmed(arguments, &format!("the Project {} and its Services", project.name), "delete_project")?;
        let ids: Vec<Id> = project.services.iter().map(|service| service.id).collect();
        self.refuse_running_elsewhere(&project, &ids, &self.live_runs())?;
        let stopped = self.stop_active(&project, &ids).await?;
        self.library.remove_project(project.id, Origin::Agent)?;
        for id in ids {
            let _ = self.targets.remove_logs(&project.execution_target, id).await;
        }
        Ok(json!({
            "removed": project_value(&project),
            "stopped": stopped,
            "note": "Removed from localhost only: the folder and its files are as they were.",
        }))
    }

    // MARK: Services

    fn create_service(&self, arguments: &Map<String, Value>) -> Result<Value, ToolError> {
        let project = self.project(arguments)?;
        let name = required_string(arguments, "name")?;
        let key = match optional_string(arguments, "key")? {
            Some(key) => Key::new(key)?,
            None => project.suggested_service_key(name, None),
        };
        let new = NewService {
            name: name.to_string(),
            key,
            working_directory: optional_string(arguments, "working_directory")?.unwrap_or(".").to_string(),
            command_line: required_string(arguments, "command_line")?.to_string(),
            port: port(arguments)?.flatten(),
            url: url(arguments)?.flatten(),
            port_can_change: optional_bool(arguments, "port_can_change")?.unwrap_or(true),
            is_excluded_from_start_all: !optional_bool(arguments, "in_start_all")?.unwrap_or(true),
        };
        let variables = variables(arguments, &Variables::default())?.unwrap_or_default();
        let dependencies = dependencies(&project, arguments)?.unwrap_or_default();
        let service = self.library.add_service(project.id, new, variables, dependencies, Origin::Agent)?;
        let project = self.library.project(project.id)?;
        Ok(json!({ "project": summary(&project), "service": service_value(&project, &service) }))
    }

    fn update_service(&self, arguments: &Map<String, Value>) -> Result<Value, ToolError> {
        let project = self.project(arguments)?;
        let service = self.service(&project, arguments)?;
        let changes = ServiceChanges {
            name: optional_string(arguments, "name")?.map(str::to_string),
            key: optional_string(arguments, "key")?.map(Key::new).transpose()?,
            working_directory: optional_string(arguments, "working_directory")?.map(str::to_string),
            command_line: optional_string(arguments, "command_line")?.map(str::to_string),
            variables: variables(arguments, &service.variables())?,
            dependencies: dependencies(&project, arguments)?,
            is_excluded_from_start_all: optional_bool(arguments, "in_start_all")?.map(|included| !included),
            port: port(arguments)?,
            url: url(arguments)?,
            port_can_change: optional_bool(arguments, "port_can_change")?,
        };
        let service = self.library.edit_service(service.id, changes, Origin::Agent)?;
        let project = self.library.project(project.id)?;
        let mut result = json!({ "project": summary(&project), "service": service_value(&project, &service) });
        if self.targets.status(service.id).is_restart_needed {
            result["note"] = format!("{} is running: restart it to apply the change.", service.key).into();
        }
        Ok(result)
    }

    async fn delete_service(&self, arguments: &Map<String, Value>) -> Result<Value, ToolError> {
        let project = self.project(arguments)?;
        let service = self.service(&project, arguments)?.clone();
        confirmed(arguments, &format!("the Service {} of {}", service.name, project.name), "delete_service")?;
        self.refuse_running_elsewhere(&project, &[service.id], &self.live_runs())?;
        let stopped = self.stop_active(&project, &[service.id]).await?;
        let dependents: Vec<&str> = project
            .services
            .iter()
            .filter(|other| other.dependencies.contains(&service.id))
            .map(|other| other.key.as_str())
            .collect();
        let removed = service_value(&project, &service);
        self.library.remove_service(service.id, Origin::Agent)?;
        let _ = self.targets.remove_logs(&project.execution_target, service.id).await;
        let mut result = json!({ "project": summary(&project), "removed": removed, "stopped": stopped });
        if !dependents.is_empty() {
            result["note"] = format!("No longer a Dependency of: {}.", dependents.join(", ")).into();
        }
        Ok(result)
    }

    /// Stops those of `service_ids` that are active; their Keys.
    async fn stop_active(&self, project: &Project, service_ids: &[Id]) -> Result<Vec<String>, ToolError> {
        let mut stopped = Vec::new();
        for service in service_ids.iter().filter_map(|id| project.service(*id)) {
            if self.targets.status(service.id).state.is_active() {
                self.targets.stop(service.id, Origin::Agent).await?;
                stopped.push(service.key.to_string());
            }
        }
        Ok(stopped)
    }
}

/// Fails unless the agent says the user agreed to remove `what`.
fn confirmed(arguments: &Map<String, Value>, what: &str, tool: &str) -> Result<(), ToolError> {
    if arguments.get("confirm") == Some(&Value::Bool(true)) {
        return Ok(());
    }
    Err(ToolError::new(
        "confirmationRequired",
        format!(
            "Nothing was removed. This removes {what} from localhost. Ask the user, then call {tool} again with confirm: true."
        ),
    ))
}

fn optional_bool(arguments: &Map<String, Value>, name: &str) -> Result<Option<bool>, ToolError> {
    match arguments.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(ToolError::invalid(format!("`{name}` must be true or false."))),
    }
}

/// `None` if left out, `Some(None)` to remove the Port.
fn port(arguments: &Map<String, Value>) -> Result<Option<Option<i64>>, ToolError> {
    match arguments.get("port") {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        // Its range is checked by the core.
        Some(port) => match port.as_i64() {
            Some(port) => Ok(Some(Some(port))),
            None => Err(ToolError::invalid("`port` must be a number from 1 to 65535, or null for none.")),
        },
    }
}

/// `None` if left out, `Some(None)` to remove the URL.
fn url(arguments: &Map<String, Value>) -> Result<Option<Option<String>>, ToolError> {
    match arguments.get("url") {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(url)) => Ok(Some(Some(url.clone()))),
        Some(_) => Err(ToolError::invalid("`url` must be a string, or null for none.")),
    }
}

/// The layer's variables from `env`: its plain values, and every Secret
/// `current` has, kept. `None` if `env` is left out.
fn variables(arguments: &Map<String, Value>, current: &Variables) -> Result<Option<Vec<VariableInput>>, ToolError> {
    let env = match arguments.get("env") {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Object(env)) => env,
        Some(_) => return Err(ToolError::invalid("`env` must be an object of variable names and values.")),
    };
    let mut inputs = Vec::new();
    for (name, value) in env {
        if current.secret_names.contains(name) {
            return Err(ToolError::invalid(format!(
                "{name} is a Secret here: only the localhost app sets it. Leave it out of `env`."
            )));
        }
        let value = match value {
            Value::String(value) => value.clone(),
            Value::Number(_) | Value::Bool(_) => value.to_string(),
            _ => return Err(ToolError::invalid(format!("The value of {name} must be a string."))),
        };
        inputs.push(VariableInput::plain(name.clone(), value));
    }
    inputs.extend(current.secret_names.iter().map(VariableInput::existing_secret));
    Ok(Some(inputs))
}

/// The Services `dependencies` names by Key or name; `None` if left out.
fn dependencies(project: &Project, arguments: &Map<String, Value>) -> Result<Option<Vec<Id>>, ToolError> {
    let names = match arguments.get("dependencies") {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Array(names)) => names,
        Some(_) => return Err(ToolError::invalid("`dependencies` must be a list of Service Keys.")),
    };
    names
        .iter()
        .map(|name| {
            let name =
                name.as_str().ok_or_else(|| ToolError::invalid("`dependencies` must be a list of Service Keys."))?;
            let service = resolve::service(project, name).map_err(|message| ToolError::new("notFound", message))?;
            Ok(service.id)
        })
        .collect::<Result<Vec<Id>, ToolError>>()
        .map(Some)
}

fn project_value(project: &Project) -> Value {
    json!({
        "id": project.id,
        "key": project.key,
        "name": project.name,
        "path": project.path,
        "favorite": project.is_favorite,
        "env": project.variables.values,
        "secrets": project.variables.secret_names,
        "services": project.services.iter().map(|service| service.key.as_str()).collect::<Vec<_>>(),
    })
}

fn service_value(project: &Project, service: &Service) -> Value {
    let key = |id: &Id| project.service(*id).map(|dependency| dependency.key.to_string());
    json!({
        "id": service.id,
        "key": service.key,
        "name": service.name,
        "commandLine": service.command_line,
        "workingDirectory": service.working_directory,
        "port": service.port,
        "url": service.url,
        "portCanChange": service.port_can_change,
        "dependencies": service.dependencies.iter().filter_map(key).collect::<Vec<_>>(),
        "inStartAll": !service.is_excluded_from_start_all,
        "env": service.environment,
        "secrets": service.secret_names,
    })
}
