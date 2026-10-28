//! The tools `localhost mcp` offers: semantic actions on Projects and
//! Services named by Key, structured results, bounded log reads.
//!
//! Every call reads the Project Configuration from disk again, so Projects
//! added or edited in the app since the agent connected are seen.
//!
//! Services run in the shared core the app drives too, so the app's Runs are
//! this session's as well (`"session": "shared"`, with who started them).
//! A `localhost up` in a terminal runs its own: through their Run records
//! this session sees them as running (`"session": "other"`), with their pid
//! and uptime, but can't tell Starting from Running or stop them. It never
//! starts a second Run of one.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use localhost_core::{
    Ansi, Id, Listener, ListenerOwner, LocalhostError, LogStore, MemorySecretStore, Origin, Orphan, OutputChannel,
    Project, ProjectLibrary, RunRegistry, Service, ServiceState, ServiceStatus, Targets,
};
use serde_json::{Map, Value, json};

use crate::resolve;
use crate::up::with_dependencies;

/// How long a start waits for its Services to settle (Running, Exited,
/// Failed) before it answers with them still Starting. A Service with a
/// Port is Running after 30 s at most, even if the Port never opens.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(45);

const DEFAULT_LOG_LINES: u64 = 200;
const MAX_LOG_LINES: u64 = 2000;

/// How many of a Failed Service's last output lines a start or status
/// includes, so the agent sees why without a second call.
const FAILURE_OUTPUT_LINES: usize = 20;

/// The definition of every tool, for `tools/list`.
pub fn definitions() -> Value {
    let project =
        json!({ "type": "string", "description": "The Project's Key (as list_projects shows it), or its name." });
    let service =
        json!({ "type": "string", "description": "The Service's Key (as list_services shows it), or its name." });
    let read_only = json!({ "readOnlyHint": true, "openWorldHint": false });
    let starts =
        json!({ "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false });
    let stops =
        json!({ "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false });
    json!([
        {
            "name": "list_projects",
            "title": "List Projects",
            "description": "Every Project localhost knows: its Key, name, folder and Services. Start here to find the Project for the folder you are working in.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": read_only,
        },
        {
            "name": "list_services",
            "title": "List Services",
            "description": "A Project's Services as configured: command line, working directory, Port, URL and Dependencies. Use status for whether they are running.",
            "inputSchema": { "type": "object", "properties": { "project": project }, "required": ["project"], "additionalProperties": false },
            "annotations": read_only,
        },
        {
            "name": "status",
            "title": "Service status",
            "description": "The state of a Project's Services (stopped, starting, running, stopping, exited, failed), with pid, Port, URL, uptime and, for a failed one, why and its last output. A Service is running only if this says so.",
            "inputSchema": {
                "type": "object",
                "properties": { "project": project, "service": { "type": "string", "description": "Only this Service. Leave out for all of them." } },
                "required": ["project"],
                "additionalProperties": false,
            },
            "annotations": read_only,
        },
        {
            "name": "start_service",
            "title": "Start a Service",
            "description": "Starts one Service, after its Dependencies, and waits until it is running, has exited or has failed (up to 45 s). Returns the status of it and its Dependencies. Does nothing to a Service already running.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "service": service,
                    "port": { "type": "integer", "minimum": 1, "maximum": 65535, "description": "Start on this port instead of the Service's Port, for this Run only. Use it when ports shows the Port is taken." },
                },
                "required": ["project", "service"],
                "additionalProperties": false,
            },
            "annotations": starts,
        },
        {
            "name": "stop_service",
            "title": "Stop a Service",
            "description": "Stops one Service and waits until its whole process tree has ended. Services that depend on it are left running.",
            "inputSchema": { "type": "object", "properties": { "project": project, "service": service }, "required": ["project", "service"], "additionalProperties": false },
            "annotations": stops,
        },
        {
            "name": "start_project",
            "title": "Start a Project",
            "description": "Starts every Service of the Project in dependency order (Start All: Services marked “Not in Start All” are skipped unless another needs them), and waits until each is running, has exited or has failed (up to 45 s).",
            "inputSchema": { "type": "object", "properties": { "project": project }, "required": ["project"], "additionalProperties": false },
            "annotations": starts,
        },
        {
            "name": "stop_project",
            "title": "Stop a Project",
            "description": "Stops every running Service of the Project, whoever started it, dependents first, and waits until they have ended.",
            "inputSchema": { "type": "object", "properties": { "project": project }, "required": ["project"], "additionalProperties": false },
            "annotations": stops,
        },
        {
            "name": "logs",
            "title": "Read logs",
            "description": "The output of the latest Run of a Project's Services, oldest first, merged by time with each line after its Service's Key, or of one Service. Bounded: the last `lines` lines (200 by default) after the filters.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "service": { "type": "string", "description": "Only this Service. Leave out for all of them." },
                    "lines": { "type": "integer", "minimum": 1, "maximum": MAX_LOG_LINES, "description": "How many of the last lines to return (default 200, at most 2000)." },
                    "since_seconds": { "type": "integer", "minimum": 1, "description": "Only lines written in the last this many seconds." },
                    "contains": { "type": "string", "description": "Only lines containing this text, ignoring case." },
                },
                "required": ["project"],
                "additionalProperties": false,
            },
            "annotations": read_only,
        },
        {
            "name": "ports",
            "title": "Who is listening",
            "description": "The processes listening on TCP ports on this machine, and who they belong to: a Service's Run, a Project's folder, or nobody (unmanaged). With a Project, only those of the Project or holding one of its Services' Ports.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Only Listeners of this Project, or holding one of its Ports." },
                    "port": { "type": "integer", "minimum": 1, "maximum": 65535, "description": "Only the Listener on this port." },
                },
                "additionalProperties": false,
            },
            "annotations": read_only,
        },
    ])
}

/// Why a tool call failed: a `kind` to match on, the message to show.
#[derive(Debug)]
struct ToolError {
    kind: String,
    message: String,
    detail: Value,
}

impl ToolError {
    fn new(kind: &str, message: impl Into<String>) -> ToolError {
        ToolError { kind: kind.into(), message: message.into(), detail: Value::Null }
    }

    fn invalid(message: impl Into<String>) -> ToolError {
        ToolError::new("invalidArguments", message)
    }
}

impl From<LocalhostError> for ToolError {
    fn from(error: LocalhostError) -> ToolError {
        let value = serde_json::to_value(&error).unwrap_or(Value::Null);
        ToolError {
            kind: value["kind"].as_str().unwrap_or("error").into(),
            message: error.message(),
            detail: value.get("detail").cloned().unwrap_or(Value::Null),
        }
    }
}

pub struct Tools {
    library: Arc<ProjectLibrary>,
    targets: Targets,
    /// Read from disk only: the shared core writes the Runs' log files.
    logs: Arc<LogStore>,
    storage_root: PathBuf,
}

impl Tools {
    pub fn new(library: Arc<ProjectLibrary>, targets: Targets, logs: Arc<LogStore>, storage_root: PathBuf) -> Tools {
        Tools { library, targets, logs, storage_root }
    }

    /// The result of `tools/call`: the tool's answer, or its failure marked
    /// `isError` so the agent reads it rather than the client hiding it.
    pub async fn call(&self, name: &str, arguments: &Value) -> Value {
        let empty = Map::new();
        let arguments = match arguments {
            Value::Object(arguments) => arguments,
            Value::Null => &empty,
            _ => return failure(ToolError::invalid("The arguments must be an object.")),
        };
        self.refresh();
        let result = match name {
            "list_projects" => Ok(self.list_projects()),
            "list_services" => self.list_services(arguments),
            "status" => self.status(arguments),
            "start_service" => self.start_service(arguments).await,
            "stop_service" => self.stop_service(arguments).await,
            "start_project" => self.start_project(arguments).await,
            "stop_project" => self.stop_project(arguments).await,
            "logs" => return self.logs(arguments).unwrap_or_else(failure),
            "ports" => self.ports(arguments).await,
            _ => Err(ToolError::new("unknownTool", format!("There is no tool “{name}”."))),
        };
        match result {
            Ok(value) => success(value),
            Err(error) => failure(error),
        }
    }

    /// Takes in the Project Configuration as stored now. A file that can't
    /// be read leaves the Projects as they were.
    fn refresh(&self) {
        if let Ok(stored) = ProjectLibrary::open(&self.storage_root, Arc::new(MemorySecretStore::new())) {
            self.library.replace_projects(stored.projects(), Origin::Agent);
        }
    }

    // MARK: Reading

    fn list_projects(&self) -> Value {
        let projects: Vec<Value> = self
            .library
            .projects()
            .iter()
            .map(|project| {
                json!({
                    "key": project.key,
                    "name": project.name,
                    "path": project.path,
                    "services": project.services.iter().map(|service| service.key.as_str()).collect::<Vec<_>>(),
                })
            })
            .collect();
        json!({ "projects": projects })
    }

    fn list_services(&self, arguments: &Map<String, Value>) -> Result<Value, ToolError> {
        let project = self.project(arguments)?;
        let key = |id: &Id| project.service(*id).map(|service| service.key.to_string());
        let services: Vec<Value> = project
            .services
            .iter()
            .map(|service| {
                json!({
                    "key": service.key,
                    "name": service.name,
                    "commandLine": service.command_line,
                    "workingDirectory": service.working_directory,
                    "port": service.port,
                    "url": service.url,
                    "dependencies": service.dependencies.iter().filter_map(key).collect::<Vec<_>>(),
                    "inStartAll": !service.is_excluded_from_start_all,
                })
            })
            .collect();
        Ok(json!({ "project": summary(&project), "services": services }))
    }

    fn status(&self, arguments: &Map<String, Value>) -> Result<Value, ToolError> {
        let project = self.project(arguments)?;
        let services: Vec<&Service> = match optional_string(arguments, "service")? {
            Some(query) => {
                vec![resolve::service(&project, query).map_err(|message| ToolError::new("notFound", message))?]
            }
            None => project.services.iter().collect(),
        };
        let ids: Vec<Id> = services.iter().map(|service| service.id).collect();
        Ok(self.statuses(&project, &ids, None))
    }

    /// Each Service's state, as this session knows it or, for one another
    /// session runs, from its Run record.
    fn statuses(&self, project: &Project, service_ids: &[Id], note: Option<String>) -> Value {
        let live = self.live_runs();
        let services: Vec<Value> = service_ids
            .iter()
            .filter_map(|id| project.service(*id))
            .map(|service| self.service_status(project, service, &live))
            .collect();
        let mut result = json!({ "project": summary(project), "services": services });
        if let Some(note) = note {
            result["note"] = note.into();
        }
        result
    }

    fn service_status(&self, project: &Project, service: &Service, live: &[Orphan]) -> Value {
        let status = self.targets.status(service.id);
        let mut value = json!({ "key": service.key, "name": service.name });
        if let Some(elsewhere) = self.elsewhere(&status, live) {
            let up = SystemTime::now().duration_since(elsewhere.started_at).unwrap_or_default();
            value["state"] = "running".into();
            value["session"] = "other".into();
            value["pid"] = elsewhere.process_group_id.into();
            value["port"] = json!(service.port);
            value["url"] = json!(service.url_to_open(None));
            value["uptimeSeconds"] = up.as_secs().into();
            value["note"] = "Run by a `localhost up` in a terminal; stop it there.".into();
            return value;
        }
        value["state"] = json!(status.state);
        let Some(run) = status.run.as_ref() else {
            value["port"] = json!(service.port);
            return value;
        };
        value["session"] = "shared".into();
        value["startedBy"] = started_by(run.origin).into();
        let is_active = status.state.is_active();
        if is_active && let Some(record) = live.iter().find(|record| record.run_id == run.id) {
            value["pid"] = record.process_group_id.into();
        }
        value["port"] = json!(run.port.or(service.port));
        if let Some(change) = &run.port_change {
            value["configuredPort"] = change.configured_port.into();
        }
        if matches!(status.state, ServiceState::Starting | ServiceState::Running) {
            value["url"] = json!(service.url_to_open(Some(run)));
        }
        value["startedAt"] = unix_seconds(run.started_at).into();
        if is_active {
            let up = SystemTime::now().duration_since(run.started_at).unwrap_or_default();
            value["uptimeSeconds"] = up.as_secs().into();
        }
        if !status.awaited_dependencies.is_empty() {
            let keys: Vec<String> = status
                .awaited_dependencies
                .iter()
                .filter_map(|id| project.service(*id))
                .map(|dependency| dependency.key.to_string())
                .collect();
            value["waitingFor"] = keys.into();
        }
        if status.is_restart_needed {
            value["restartNeeded"] = true.into();
        }
        if run.is_port_unresponsive {
            value["portUnresponsive"] = true.into();
        }
        if let Some(termination) = run.termination.filter(|_| !is_active) {
            value["exit"] = termination.summary().into();
        }
        if status.state == ServiceState::Failed {
            let error = run.failure.clone().map(ToolError::from);
            value["failure"] = match error {
                Some(error) => json!({ "kind": error.kind, "message": error.message, "detail": error.detail }),
                None => json!({ "message": run.termination.map(|termination| termination.summary()) }),
            };
            // The latest Run file is this Run's: it ended, and none started since.
            let output: Vec<String> = self
                .logs
                .stored_lines(service.id)
                .into_iter()
                .filter(|line| line.channel != OutputChannel::Localhost)
                .map(|line| Ansi::strip(&line.text))
                .collect();
            let output = &output[output.len().saturating_sub(FAILURE_OUTPUT_LINES)..];
            value["lastOutput"] = output.into();
        }
        value
    }

    // MARK: Starting and stopping

    async fn start_service(&self, arguments: &Map<String, Value>) -> Result<Value, ToolError> {
        let project = self.project(arguments)?;
        let service = self.service(&project, arguments)?;
        let port = match arguments.get("port") {
            None | Some(Value::Null) => None,
            Some(port) => Some(
                port.as_u64()
                    .and_then(|port| u16::try_from(port).ok())
                    .filter(|port| *port > 0)
                    .ok_or_else(|| ToolError::invalid("`port` must be a number from 1 to 65535."))?,
            ),
        };
        let starting = with_dependencies(&project, vec![service.id]);
        let live = self.live_runs();
        if self.elsewhere(&self.targets.status(service.id), &live).is_some() {
            let note = format!(
                "{} is already running in another localhost session (a `localhost up`); it was left as it is.",
                service.key
            );
            return Ok(self.statuses(&project, &starting, Some(note)));
        }
        self.refuse_running_elsewhere(&project, &starting, &live)?;
        self.targets.start(service.id, port, Origin::Agent).await?;
        self.settle(&starting).await;
        Ok(self.statuses(&project, &starting, self.still_starting(&project, &starting)))
    }

    async fn stop_service(&self, arguments: &Map<String, Value>) -> Result<Value, ToolError> {
        let project = self.project(arguments)?;
        let service = self.service(&project, arguments)?;
        let status = self.targets.status(service.id);
        if let Some(elsewhere) = self.elsewhere(&status, &self.live_runs()) {
            return Err(running_elsewhere(service, elsewhere));
        }
        self.targets.stop(service.id, Origin::Agent).await?;
        Ok(self.statuses(&project, &[service.id], None))
    }

    async fn start_project(&self, arguments: &Map<String, Value>) -> Result<Value, ToolError> {
        let project = self.project(arguments)?;
        let included: Vec<Id> =
            project.services.iter().filter(|service| !service.is_excluded_from_start_all).map(|s| s.id).collect();
        let starting = with_dependencies(&project, included);
        if starting.is_empty() {
            return Err(ToolError::new("nothingToStart", format!("{} has no Services to start.", project.name)));
        }
        self.refuse_running_elsewhere(&project, &starting, &self.live_runs())?;
        self.targets.start_all(project.id, Origin::Agent).await?;
        self.settle(&starting).await;
        Ok(self.statuses(&project, &starting, self.still_starting(&project, &starting)))
    }

    async fn stop_project(&self, arguments: &Map<String, Value>) -> Result<Value, ToolError> {
        let project = self.project(arguments)?;
        self.targets.stop_all(project.id, Origin::Agent).await?;
        let live = self.live_runs();
        let elsewhere: Vec<&str> = project
            .services
            .iter()
            .filter(|service| self.elsewhere(&self.targets.status(service.id), &live).is_some())
            .map(|service| service.key.as_str())
            .collect();
        let note = (!elsewhere.is_empty())
            .then(|| format!("Left running, because another localhost session runs them: {}.", elsewhere.join(", ")));
        let all: Vec<Id> = project.services.iter().map(|service| service.id).collect();
        Ok(self.statuses(&project, &all, note))
    }

    /// Fails if another session runs any of `service_ids`: a second Run
    /// would only fight over its Port and its files.
    fn refuse_running_elsewhere(
        &self,
        project: &Project,
        service_ids: &[Id],
        live: &[Orphan],
    ) -> Result<(), ToolError> {
        for service in service_ids.iter().filter_map(|id| project.service(*id)) {
            if let Some(elsewhere) = self.elsewhere(&self.targets.status(service.id), live) {
                return Err(running_elsewhere(service, elsewhere));
            }
        }
        Ok(())
    }

    /// Waits until none of the Services is Starting, or `SETTLE_TIMEOUT`.
    async fn settle(&self, service_ids: &[Id]) {
        let settled = async {
            while service_ids.iter().any(|id| self.targets.status(*id).state == ServiceState::Starting) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        };
        let _ = tokio::time::timeout(SETTLE_TIMEOUT, settled).await;
    }

    fn still_starting(&self, project: &Project, service_ids: &[Id]) -> Option<String> {
        let keys: Vec<&str> = service_ids
            .iter()
            .filter(|id| self.targets.status(**id).state == ServiceState::Starting)
            .filter_map(|id| project.service(*id))
            .map(|service| service.key.as_str())
            .collect();
        (!keys.is_empty()).then(|| format!("Still starting: {}. Call status again shortly.", keys.join(", ")))
    }

    // MARK: Logs and ports

    fn logs(&self, arguments: &Map<String, Value>) -> Result<Value, ToolError> {
        let project = self.project(arguments)?;
        let services: Vec<&Service> = match optional_string(arguments, "service")? {
            Some(query) => {
                vec![resolve::service(&project, query).map_err(|message| ToolError::new("notFound", message))?]
            }
            None => project.services.iter().collect(),
        };
        let limit = match arguments.get("lines") {
            None | Some(Value::Null) => DEFAULT_LOG_LINES,
            Some(lines) => lines
                .as_u64()
                .filter(|lines| (1..=MAX_LOG_LINES).contains(lines))
                .ok_or_else(|| ToolError::invalid(format!("`lines` must be a number from 1 to {MAX_LOG_LINES}.")))?,
        } as usize;
        let since = match arguments.get("since_seconds") {
            None | Some(Value::Null) => None,
            Some(seconds) => {
                let seconds = seconds.as_u64().filter(|seconds| *seconds > 0);
                let seconds =
                    seconds.ok_or_else(|| ToolError::invalid("`since_seconds` must be a positive number."))?;
                Some(SystemTime::now() - Duration::from_secs(seconds))
            }
        };
        let contains = optional_string(arguments, "contains")?.map(str::to_lowercase);

        let mut lines: Vec<(&Service, OutputChannel, SystemTime, String)> = services
            .iter()
            .flat_map(|service| {
                self.logs
                    .stored_lines(service.id)
                    .into_iter()
                    .map(move |line| (*service, line.channel, line.timestamp, Ansi::strip(&line.text)))
            })
            .filter(|(_, _, timestamp, _)| since.is_none_or(|since| *timestamp >= since))
            .filter(|(_, _, _, text)| contains.as_ref().is_none_or(|needle| text.to_lowercase().contains(needle)))
            .collect();
        // Stable, so each Service's lines keep their order.
        lines.sort_by_key(|(_, _, timestamp, _)| *timestamp);
        let matched = lines.len();
        let lines = &lines[matched.saturating_sub(limit)..];

        let is_merged = services.len() > 1;
        let width = crate::logs::prefix_width(lines.iter().map(|(service, ..)| *service));
        let text: Vec<String> = lines
            .iter()
            .map(|(service, _, _, text)| match is_merged {
                true => format!("{:<width$} | {text}", service.key.as_str()),
                false => text.clone(),
            })
            .collect();
        let structured: Vec<Value> = lines
            .iter()
            .map(|(service, channel, timestamp, text)| {
                json!({ "service": service.key, "channel": channel, "time": unix_seconds(*timestamp), "text": text })
            })
            .collect();
        let shown = lines.len();
        let header = match (shown, matched) {
            (0, _) => "No log lines match.".to_string(),
            (shown, matched) if shown < matched => format!("The last {shown} of {matched} lines:"),
            (shown, _) => format!("{shown} lines:"),
        };
        let text = std::iter::once(header).chain(text).collect::<Vec<_>>().join("\n");
        Ok(json!({
            "content": [{ "type": "text", "text": text }],
            "structuredContent": { "project": summary(&project), "lines": structured, "matched": matched },
            "isError": false,
        }))
    }

    async fn ports(&self, arguments: &Map<String, Value>) -> Result<Value, ToolError> {
        let project = match optional_string(arguments, "project")? {
            Some(query) => Some(
                resolve::project(&self.library.projects(), query)
                    .map_err(|message| ToolError::new("notFound", message))?,
            ),
            None => None,
        };
        let port = match arguments.get("port") {
            None | Some(Value::Null) => None,
            Some(port) => Some(
                port.as_u64()
                    .and_then(|port| u16::try_from(port).ok())
                    .ok_or_else(|| ToolError::invalid("`port` must be a number from 1 to 65535."))?,
            ),
        };
        let projects = self.library.projects();
        let live = self.live_runs();
        let listeners: Vec<Value> = self
            .targets
            .listeners()
            .await
            .iter()
            .map(|(_, listener)| listener)
            .filter(|listener| port.is_none_or(|port| listener.ports.contains(&port)))
            .filter_map(|listener| {
                let value = self.listener(listener, &projects, &live);
                let in_project = project.as_ref().is_none_or(|project| {
                    value["project"] == json!(project.key)
                        || project
                            .services
                            .iter()
                            .any(|service| service.port.is_some_and(|port| listener.ports.contains(&port)))
                });
                in_project.then_some(value)
            })
            .collect();
        Ok(json!({ "listeners": listeners }))
    }

    fn listener(&self, listener: &Listener, projects: &[Project], live: &[Orphan]) -> Value {
        let service_of =
            |service_id: Id| projects.iter().find_map(|project| Some((project, project.service(service_id)?)));
        let (project, service, session, owner) = match listener.owner {
            ListenerOwner::Run { service_id, .. } => {
                let found = service_of(service_id);
                (found.map(|(project, _)| project), found.map(|(_, service)| service), Some("shared"), "run")
            }
            _ => match live
                .iter()
                .find(|run| run.process_group_id == listener.process_group_id)
                .and_then(|run| service_of(run.service_id))
            {
                Some((project, service)) => (Some(project), Some(service), Some("other"), "run"),
                None => match listener.owner {
                    ListenerOwner::Project { project_id } => {
                        (projects.iter().find(|project| project.id == project_id), None, None, "project")
                    }
                    _ => (None, None, None, "unmanaged"),
                },
            },
        };
        let holds: Vec<String> = projects
            .iter()
            .flat_map(|project| project.services.iter().map(move |service| (project, service)))
            .filter(|(_, held)| held.port.is_some_and(|port| listener.ports.contains(&port)))
            .filter(|(_, held)| service.is_none_or(|service| service.id != held.id))
            .map(|(project, held)| format!("{}/{}", project.key, held.key))
            .collect();
        let mut value = json!({
            "ports": listener.ports,
            "pid": listener.pid,
            "processGroupId": listener.process_group_id,
            "name": listener.name,
            "command": listener.command,
            "workingDirectory": listener.working_directory,
            "owner": owner,
            "kind": listener.kind,
            "description": listener.owner_description(projects),
        });
        if let Some(project) = project {
            value["project"] = json!(project.key);
        }
        if let Some(service) = service {
            value["service"] = json!(service.key);
            value["description"] =
                format!("{} › {}", project.map_or("", |project| project.name.as_str()), service.name).into();
        }
        if let Some(session) = session {
            value["session"] = session.into();
        }
        if !holds.is_empty() {
            value["holdsPortOf"] = holds.into();
        }
        value
    }

    // MARK: Lookup

    fn project(&self, arguments: &Map<String, Value>) -> Result<Project, ToolError> {
        let query = required_string(arguments, "project")?;
        resolve::project(&self.library.projects(), query).map_err(|message| ToolError::new("notFound", message))
    }

    fn service<'a>(&self, project: &'a Project, arguments: &Map<String, Value>) -> Result<&'a Service, ToolError> {
        let query = required_string(arguments, "service")?;
        resolve::service(project, query).map_err(|message| ToolError::new("notFound", message))
    }

    /// Every live Run's record, this session's included: a registry of its
    /// own has no Runs of this session to leave out.
    fn live_runs(&self) -> Vec<Orphan> {
        RunRegistry::new(&self.storage_root).orphans()
    }

    /// The Run another session has of this Service, unless this session's
    /// is active.
    fn elsewhere<'a>(&self, status: &ServiceStatus, live: &'a [Orphan]) -> Option<&'a Orphan> {
        if status.state.is_active() {
            return None;
        }
        let ours = status.run.as_ref().map(|run| run.id);
        live.iter().find(|run| run.service_id == status.service_id && Some(run.run_id) != ours)
    }
}

fn running_elsewhere(service: &Service, run: &Orphan) -> ToolError {
    ToolError {
        kind: "runningInAnotherSession".into(),
        message: format!(
            "{} is running in another localhost session (pid {}): a `localhost up` in a terminal. Stop it there first, or use it as it is.",
            service.key, run.process_group_id
        ),
        detail: json!({ "service": service.key, "pid": run.process_group_id }),
    }
}

/// Who started a Run, as an agent reads it.
fn started_by(origin: Origin) -> &'static str {
    match origin {
        Origin::MainWindow | Origin::MenuBar => "app",
        Origin::Cli => "cli",
        Origin::Agent => "agent",
    }
}

fn summary(project: &Project) -> Value {
    json!({ "key": project.key, "name": project.name, "path": project.path })
}

fn required_string<'a>(arguments: &'a Map<String, Value>, name: &str) -> Result<&'a str, ToolError> {
    optional_string(arguments, name)?.ok_or_else(|| ToolError::invalid(format!("`{name}` is required.")))
}

fn optional_string<'a>(arguments: &'a Map<String, Value>, name: &str) -> Result<Option<&'a str>, ToolError> {
    match arguments.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if !text.trim().is_empty() => Ok(Some(text.trim())),
        Some(_) => Err(ToolError::invalid(format!("`{name}` must be a non-empty string."))),
    }
}

fn unix_seconds(time: SystemTime) -> f64 {
    let seconds = time.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64();
    (seconds * 1000.0).round() / 1000.0
}

fn success(value: Value) -> Value {
    let text = serde_json::to_string_pretty(&value).unwrap_or_default();
    json!({ "content": [{ "type": "text", "text": text }], "structuredContent": value, "isError": false })
}

fn failure(error: ToolError) -> Value {
    let structured = json!({ "error": { "kind": error.kind, "message": error.message, "detail": error.detail } });
    json!({ "content": [{ "type": "text", "text": error.message }], "structuredContent": structured, "isError": true })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn every_tool_has_a_name_a_description_and_an_object_schema() {
        let definitions = definitions();
        let tools = definitions.as_array().unwrap();
        let names: HashSet<&str> = tools.iter().map(|tool| tool["name"].as_str().unwrap()).collect();
        assert_eq!(names.len(), tools.len());
        for name in [
            "list_projects",
            "list_services",
            "status",
            "start_service",
            "stop_service",
            "start_project",
            "stop_project",
            "logs",
            "ports",
        ] {
            assert!(names.contains(name), "{name}");
        }
        for tool in tools {
            assert!(!tool["description"].as_str().unwrap().is_empty());
            assert_eq!(tool["inputSchema"]["type"], "object");
        }
    }

    #[test]
    fn core_errors_keep_their_kind_and_detail() {
        let error = ToolError::from(LocalhostError::PortConflict { port: 3000, owner: None });
        assert_eq!(error.kind, "portConflict");
        assert_eq!(error.detail["port"], 3000);
        assert_eq!(error.message, "Port 3000 is in use by another process.");

        let reply = failure(error);
        assert_eq!(reply["isError"], true);
        assert_eq!(reply["structuredContent"]["error"]["kind"], "portConflict");
    }

    #[test]
    fn string_arguments_are_trimmed_and_checked() {
        let arguments: Map<String, Value> =
            serde_json::from_value(json!({ "project": " acme ", "service": 3, "blank": "" })).unwrap();
        assert_eq!(required_string(&arguments, "project").unwrap(), "acme");
        assert_eq!(required_string(&arguments, "service").unwrap_err().kind, "invalidArguments");
        assert!(optional_string(&arguments, "blank").is_err());
        assert_eq!(optional_string(&arguments, "missing").unwrap(), None);
        assert_eq!(required_string(&arguments, "missing").unwrap_err().message, "`missing` is required.");
    }
}
