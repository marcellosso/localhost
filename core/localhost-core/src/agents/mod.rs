//! Connecting coding agents to `localhost mcp`: the `localhost` MCP server
//! in each agent's configuration file and the localhost-mcp skill in its
//! skills folder, at the places each agent documents. Nothing is written
//! until the user asks, only localhost's own entries are ever changed, and
//! each file is backed up before localhost first edits it.
//!
//! The documented locations:
//! - Claude Code: user-scope MCP servers in `~/.claude.json` (top-level
//!   `mcpServers`), personal skills in `~/.claude/skills/<name>/SKILL.md`.
//!   <https://code.claude.com/docs/en/mcp>, <https://code.claude.com/docs/en/skills>
//! - Cursor: global MCP servers in `~/.cursor/mcp.json`, user skills in
//!   `~/.cursor/skills/<name>/SKILL.md`.
//!   <https://cursor.com/docs/context/mcp>, <https://cursor.com/docs/skills>
//! - Codex: `[mcp_servers.<name>]` in `~/.codex/config.toml`, shared by the
//!   CLI, the IDE extension and the app; user skills in `~/.agents/skills`.
//!   <https://developers.openai.com/codex/mcp>, <https://developers.openai.com/codex/skills>

mod command_line;
mod files;
mod json_edit;
mod toml_edit;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub use command_line::{CommandLineState, CommandLineStatus};

use crate::LocalhostError;
use files::{display, exists, join};

/// The skill agents load to run Services through localhost, as the
/// repository ships it.
pub const AGENT_SKILL: &str = include_str!("../../../../skills/localhost-mcp/SKILL.md");

const SERVER: &str = "localhost";
const SKILL_FOLDER: &str = "localhost-mcp";
const JSON_PATH: [&str; 2] = ["mcpServers", SERVER];
const TOML_PATH: [&str; 2] = ["mcp_servers", SERVER];
/// What the `localhost` server runs: `localhost mcp`, or with the tools
/// that change the Project Configuration left out, which a user may choose
/// by hand and Connect keeps.
const ARGUMENTS: &[&str] = &["mcp"];
const READ_ONLY_ARGUMENTS: &[&str] = &["mcp", "--read-only"];

/// A coding agent localhost can connect to its MCP server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum AgentClient {
    ClaudeCode,
    Cursor,
    Codex,
}

impl AgentClient {
    pub const ALL: [AgentClient; 3] = [AgentClient::ClaudeCode, AgentClient::Cursor, AgentClient::Codex];

    pub fn name(self) -> &'static str {
        match self {
            AgentClient::ClaudeCode => "Claude Code",
            AgentClient::Cursor => "Cursor",
            AgentClient::Codex => "Codex",
        }
    }

    fn slug(self) -> &'static str {
        match self {
            AgentClient::ClaudeCode => "claude-code",
            AgentClient::Cursor => "cursor",
            AgentClient::Codex => "codex",
        }
    }

    /// The folder whose presence says the agent is installed.
    fn folder(self, home: &Path) -> PathBuf {
        match self {
            AgentClient::ClaudeCode => home.join(".claude"),
            AgentClient::Cursor => home.join(".cursor"),
            AgentClient::Codex => home.join(".codex"),
        }
    }

    fn config_path(self, home: &Path) -> PathBuf {
        match self {
            AgentClient::ClaudeCode => home.join(".claude.json"),
            AgentClient::Cursor => join(home, &[".cursor", "mcp.json"]),
            AgentClient::Codex => join(home, &[".codex", "config.toml"]),
        }
    }

    fn skill_path(self, home: &Path) -> PathBuf {
        let skills = match self {
            AgentClient::ClaudeCode => join(home, &[".claude", "skills"]),
            AgentClient::Cursor => join(home, &[".cursor", "skills"]),
            AgentClient::Codex => join(home, &[".agents", "skills"]),
        };
        join(&skills, &[SKILL_FOLDER, "SKILL.md"])
    }

    fn is_installed(self, home: &Path) -> bool {
        exists(&self.folder(home)) || (self == AgentClient::ClaudeCode && exists(&self.config_path(home)))
    }

    /// When the agent reads its MCP servers.
    fn next_step(self) -> &'static str {
        match self {
            AgentClient::ClaudeCode => "New Claude Code sessions can use localhost.",
            AgentClient::Cursor => "Cursor picks it up; check Cursor Settings › MCP.",
            AgentClient::Codex => "Restart Codex to use localhost.",
        }
    }
}

/// Where an agent stands with localhost.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum AgentConnection {
    /// Its configuration folder isn't on this machine.
    NotInstalled,
    /// It has no `localhost` MCP server.
    NotConnected,
    /// Its `localhost` server runs this app's command, and the skill is installed.
    Connected,
    /// Its `localhost` server runs another `localhost` command, one that is
    /// gone, or the skill is missing or older: Connect updates it.
    NeedsUpdate,
    /// Its configuration file can't be read, or sets the server in a way
    /// localhost doesn't edit.
    Unreadable,
}

/// One agent, as the Connect screen shows it: where it stands, and exactly
/// what Connect would write.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AgentStatus {
    pub client: AgentClient,
    pub name: String,
    pub connection: AgentConnection,
    /// What is the case and what to do next, in a sentence or two.
    pub summary: String,
    /// The configuration file holding its MCP servers.
    pub config_path: String,
    /// The `localhost` server as Connect writes it in that file.
    pub entry: String,
    /// The command Connect and Disconnect run to change that file, when the
    /// agent has one (Claude Code's `claude mcp`), instead of editing it.
    pub via_command: Option<String>,
    /// Where Connect installs the localhost-mcp skill.
    pub skill_path: String,
    /// The agent whose skill folder, at `skill_path`, this one reads the
    /// skill from instead of a copy of its own (Cursor reads Claude Code's
    /// and Codex's).
    pub skill_shared_from: Option<String>,
    /// The command its `localhost` server runs now, if it has one.
    pub configured_command: Option<String>,
    pub is_skill_installed: bool,
    /// The configuration file as it was before localhost first changed it.
    pub backup_path: Option<String>,
}

/// Connects agents to one `localhost` command: the one bundled with the app.
#[cfg_attr(feature = "ffi", derive(uniffi::Object))]
pub struct AgentSetup {
    home: PathBuf,
    command: PathBuf,
    backups: PathBuf,
    /// `PATH` as a new terminal sees it, read once.
    terminal_path: OnceLock<Option<String>>,
    claude: Claude,
}

/// How Claude Code's servers are changed: through its `claude` command when
/// there is one, which knows how to write its own file, else by editing
/// `~/.claude.json`.
enum Claude {
    /// Look for `claude` on the terminal's `PATH` and where its installers put it.
    Find,
    /// This `claude` (a test's stand-in), or none: edit the file.
    Given(Option<PathBuf>),
}

impl AgentSetup {
    /// Agents of this user, pointed at `command`; first-edit backups go in
    /// `storage_root`'s `agent-backups` folder.
    pub fn new(command: impl Into<PathBuf>, storage_root: &Path) -> Self {
        Self { claude: Claude::Find, ..Self::with_home(crate::paths::home_directory(), command, storage_root) }
    }

    /// As `new`, for agents configured under `home` instead of the user's
    /// home folder. It never runs the user's `claude`, which would change
    /// their own configuration, not `home`'s: see `with_claude_command`.
    pub fn with_home(home: impl Into<PathBuf>, command: impl Into<PathBuf>, storage_root: &Path) -> Self {
        Self {
            home: home.into(),
            command: command.into(),
            backups: storage_root.join("agent-backups"),
            terminal_path: OnceLock::new(),
            claude: Claude::Given(None),
        }
    }

    /// As `with_home`, changing Claude Code's servers through `claude`.
    pub fn with_claude_command(self, claude: Option<PathBuf>) -> Self {
        Self { claude: Claude::Given(claude), ..self }
    }

    /// As `with_home`, with the `PATH` new terminals get, instead of asking the login shell.
    pub fn with_terminal_path(self, path: Option<String>) -> Self {
        let _ = self.terminal_path.set(path);
        self
    }

    /// The command agents are pointed at.
    pub fn command(&self) -> &Path {
        &self.command
    }

    pub fn statuses(&self) -> Vec<AgentStatus> {
        AgentClient::ALL.iter().map(|client| self.status(*client)).collect()
    }

    pub fn status(&self, client: AgentClient) -> AgentStatus {
        let config_path = client.config_path(&self.home);
        let skill_path = client.skill_path(&self.home);
        let backup = self.backup_path(client);
        let mut status = AgentStatus {
            client,
            name: client.name().into(),
            connection: AgentConnection::NotConnected,
            summary: String::new(),
            config_path: config_path.display().to_string(),
            entry: self.entry_text(client),
            via_command: (client == AgentClient::ClaudeCode && self.claude_command().is_some())
                .then(|| "claude mcp add-json --scope user localhost".to_string()),
            skill_path: skill_path.display().to_string(),
            configured_command: None,
            skill_shared_from: None,
            is_skill_installed: files::read(&skill_path).ok().flatten().is_some_and(|text| is_our_skill(&text)),
            backup_path: exists(&backup).then(|| backup.display().to_string()),
        };
        let shared_from = (client == AgentClient::Cursor).then(|| self.shared_skill_for_cursor()).flatten();
        if let Some((from, path)) = &shared_from {
            status.skill_path = path.display().to_string();
            status.skill_shared_from = Some(from.name().into());
            status.is_skill_installed = true;
        }
        if !client.is_installed(&self.home) {
            status.connection = AgentConnection::NotInstalled;
            status.summary =
                format!("Not found on this computer: there is no {}.", display(&client.folder(&self.home), &self.home));
            return status;
        }
        let entry = match self.read_entry(client) {
            Ok(entry) => entry,
            Err(reason) => {
                status.connection = AgentConnection::Unreadable;
                status.summary = format!(
                    "localhost can’t edit {}: {reason}. Fix the file, or set up the server by hand.",
                    display(&config_path, &self.home)
                );
                return status;
            }
        };
        let Some(entry) = entry else {
            status.summary = "Not connected.".into();
            return status;
        };
        status.configured_command = entry.command.clone();
        let is_current_skill =
            shared_from.is_some() || files::read(&skill_path).ok().flatten().is_some_and(|text| text == AGENT_SKILL);
        let problem = match &entry.command {
            Some(command) if Path::new(command) != self.command => {
                let shown = display(Path::new(command), &self.home);
                if Path::new(command).is_absolute() && !exists(Path::new(command)) {
                    Some(format!("It runs {shown}, which no longer exists. Update it to use this app’s command."))
                } else {
                    Some(format!("It runs another localhost command, {shown}. Update it to use this app’s."))
                }
            }
            None => Some("Its localhost server has no command. Update it to fix it.".into()),
            Some(_) if !entry.has_mcp_arguments => {
                Some("Its localhost server doesn’t run `localhost mcp`. Update it to fix it.".into())
            }
            Some(_) if !status.is_skill_installed => {
                Some("The localhost-mcp skill isn’t installed. Update to add it.".into())
            }
            Some(_) if !is_current_skill => {
                Some("The localhost-mcp skill has changed. Update to install this version.".into())
            }
            Some(_) => None,
        };
        match problem {
            Some(problem) => {
                status.connection = AgentConnection::NeedsUpdate;
                status.summary = problem;
            }
            None => {
                status.connection = AgentConnection::Connected;
                status.summary = format!("Connected. {}", client.next_step());
                if entry.is_read_only {
                    status.summary += " It is read-only: it can’t add, edit or remove Projects and Services.";
                }
                if let Some((from, path)) = shared_from {
                    let folder = display(path.parent().and_then(Path::parent).unwrap_or(&path), &self.home);
                    status.summary += &format!(
                        " It reads the localhost-mcp skill from {folder}, where {} has it, so it isn’t installed twice.",
                        from.name()
                    );
                }
            }
        }
        status
    }

    /// Adds or updates the `localhost` server in the agent's configuration,
    /// pointed at this app's command, and installs the skill. Running it
    /// again changes nothing.
    pub fn connect(&self, client: AgentClient) -> Result<AgentStatus, LocalhostError> {
        if !client.is_installed(&self.home) {
            return Err(LocalhostError::AgentNotInstalled(client.name().into()));
        }
        self.check_command()?;
        let path = client.config_path(&self.home);
        let original = files::read(&path)?;
        let text = original.as_deref().unwrap_or("");
        let command = self.command.to_string_lossy();
        let arguments = self.arguments(client);
        if client == AgentClient::ClaudeCode
            && let Some(claude) = self.claude_command()
        {
            self.connect_with_claude(&claude, text, &path, arguments)?;
            self.install_skill(client)?;
            return Ok(self.status(client));
        }
        let edited = match client {
            AgentClient::Codex => toml_edit::set(
                text,
                &TOML_PATH,
                &[("command", toml_edit::render_string(&command)), ("args", toml_edit::render_strings(arguments))],
            ),
            AgentClient::ClaudeCode | AgentClient::Cursor => {
                match json_edit::get(text, &JSON_PATH) {
                    // A stdio server of the user's: only what runs it changes, its `env` and the rest stay.
                    Ok(Some(Value::Object(entry)))
                        if !entry.contains_key("url") && entry.get("type").is_none_or(|kind| kind == "stdio") =>
                    {
                        json_edit::set(text, &[JSON_PATH[0], SERVER, "command"], &json!(command))
                            .and_then(|text| json_edit::set(&text, &[JSON_PATH[0], SERVER, "args"], &json!(arguments)))
                    }
                    Ok(_) => json_edit::set(text, &JSON_PATH, &self.entry()),
                    Err(reason) => Err(reason),
                }
            }
        }
        .map_err(|reason| self.unsupported(&path, reason))?;
        if original.as_deref() != Some(edited.as_str()) {
            files::back_up(&path, &self.backup_path(client))?;
            files::write(&path, &edited)?;
        }
        self.install_skill(client)?;
        Ok(self.status(client))
    }

    /// Adds or repoints Claude Code's `localhost` server with `claude mcp`:
    /// `add-json` refuses a name that exists, so a server to repoint is
    /// removed first, then added back with its other keys (`env`, …).
    fn connect_with_claude(
        &self,
        claude: &Path,
        text: &str,
        path: &Path,
        arguments: &[&str],
    ) -> Result<(), LocalhostError> {
        let current = json_edit::get(text, &JSON_PATH).ok().flatten();
        let mut desired = serde_json::to_value(self.entry()).expect("an entry serializes");
        if let Some(Value::Object(entry)) = &current
            && !entry.contains_key("url")
            && entry.get("type").is_none_or(|kind| kind == "stdio")
        {
            let mut merged = entry.clone();
            merged.insert("command".into(), json!(self.command.to_string_lossy()));
            merged.insert("args".into(), json!(arguments));
            desired = Value::Object(merged);
        }
        if current.as_ref() == Some(&desired) {
            return Ok(());
        }
        files::back_up(path, &self.backup_path(AgentClient::ClaudeCode))?;
        if current.is_some() {
            run_claude(claude, &["mcp", "remove", "--scope", "user", SERVER], self.terminal_path())?;
        }
        let json = desired.to_string();
        run_claude(claude, &["mcp", "add-json", "--scope", "user", SERVER, &json], self.terminal_path())
    }

    /// Writes the skill where the agent reads it, unless Cursor reads it
    /// already from another agent's folder; keeps one copy Cursor sees.
    fn install_skill(&self, client: AgentClient) -> Result<(), LocalhostError> {
        let cursor_own = AgentClient::Cursor.skill_path(&self.home);
        if client == AgentClient::Cursor && self.shared_skill_for_cursor().is_some() {
            if files::read(&cursor_own)?.is_some_and(|text| is_our_skill(&text)) {
                files::remove_with_folder(&cursor_own)?;
            }
            return Ok(());
        }
        let skill = client.skill_path(&self.home);
        if files::read(&skill)?.as_deref() != Some(AGENT_SKILL) {
            files::write(&skill, AGENT_SKILL)?;
        }
        // Cursor reads this folder too: its own copy would be a second one.
        if client != AgentClient::Cursor && files::read(&cursor_own)?.is_some_and(|text| is_our_skill(&text)) {
            files::remove_with_folder(&cursor_own)?;
        }
        Ok(())
    }

    /// The localhost-mcp skill Cursor already reads from Claude Code's or
    /// Codex's folder, which Cursor documents loading
    /// (<https://cursor.com/docs/skills>): this version, installed by localhost.
    fn shared_skill_for_cursor(&self) -> Option<(AgentClient, PathBuf)> {
        [AgentClient::ClaudeCode, AgentClient::Codex].into_iter().find_map(|client| {
            let path = client.skill_path(&self.home);
            (files::read(&path).ok().flatten().as_deref() == Some(AGENT_SKILL)).then_some((client, path))
        })
    }

    /// Removes the `localhost` server from the agent's configuration, and
    /// the skill if it is localhost's. Everything else stays.
    pub fn disconnect(&self, client: AgentClient) -> Result<AgentStatus, LocalhostError> {
        let path = client.config_path(&self.home);
        let claude = (client == AgentClient::ClaudeCode).then(|| self.claude_command()).flatten();
        if let Some(claude) = claude {
            let text = files::read(&path)?.unwrap_or_default();
            if json_edit::get(&text, &JSON_PATH).ok().flatten().is_some() {
                files::back_up(&path, &self.backup_path(client))?;
                run_claude(&claude, &["mcp", "remove", "--scope", "user", SERVER], self.terminal_path())?;
            }
        } else if let Some(text) = files::read(&path)? {
            let edited = match client {
                AgentClient::Codex => toml_edit::remove(&text, &TOML_PATH),
                AgentClient::ClaudeCode | AgentClient::Cursor => json_edit::remove(&text, &JSON_PATH),
            }
            .map_err(|reason| self.unsupported(&path, reason))?;
            if let Some(edited) = edited {
                files::back_up(&path, &self.backup_path(client))?;
                files::write(&path, &edited)?;
            }
        }
        let skill = client.skill_path(&self.home);
        if files::read(&skill)?.is_some_and(|text| is_our_skill(&text)) {
            files::remove_with_folder(&skill)?;
        }
        // Cursor read that copy: a connected Cursor gets its own again.
        if client != AgentClient::Cursor && self.read_entry(AgentClient::Cursor).is_ok_and(|entry| entry.is_some()) {
            self.install_skill(AgentClient::Cursor)?;
        }
        Ok(self.status(client))
    }

    /// The `claude` command to change Claude Code's servers with, if any.
    fn claude_command(&self) -> Option<PathBuf> {
        match &self.claude {
            Claude::Given(claude) => claude.clone(),
            Claude::Find => {
                let name = if cfg!(windows) { "claude.exe" } else { "claude" };
                let separator = if cfg!(windows) { ';' } else { ':' };
                let on_path = self.terminal_path().unwrap_or_default();
                // Where the native installer and `claude migrate-installer` put it, on the PATH or not.
                let installed =
                    [join(&self.home, &[".local", "bin", name]), join(&self.home, &[".claude", "local", name])];
                on_path
                    .split(separator)
                    .filter(|folder| !folder.is_empty())
                    .map(|folder| Path::new(folder).join(name))
                    .chain(installed)
                    .find(|path| path.is_file())
            }
        }
    }

    fn unsupported(&self, path: &Path, reason: String) -> LocalhostError {
        LocalhostError::AgentConfigUnsupported { path: display(path, &self.home), reason }
    }

    fn backup_path(&self, client: AgentClient) -> PathBuf {
        let name = client.config_path(&self.home).file_name().map(|name| name.to_os_string()).unwrap_or_default();
        self.backups.join(client.slug()).join(name)
    }

    /// Fails when agents couldn't start the command later: a development
    /// build without it, or an app run from where macOS put it temporarily.
    fn check_command(&self) -> Result<(), LocalhostError> {
        let shown = self.command.display().to_string();
        if !self.command.is_absolute() || !self.command.is_file() {
            return Err(LocalhostError::CommandNotBundled(shown));
        }
        if cfg!(target_os = "macos") && (shown.contains("/AppTranslocation/") || shown.starts_with("/Volumes/")) {
            return Err(LocalhostError::CommandInTemporaryLocation(shown));
        }
        Ok(())
    }

    fn entry(&self) -> ServerEntry {
        ServerEntry { kind: "stdio", command: self.command.to_string_lossy().into_owned(), args: ARGUMENTS }
    }

    /// What Connect points the agent's server at: read-only if it is now.
    fn arguments(&self, client: AgentClient) -> &'static [&'static str] {
        match self.read_entry(client) {
            Ok(Some(entry)) if entry.is_read_only => READ_ONLY_ARGUMENTS,
            _ => ARGUMENTS,
        }
    }

    /// The entry as it reads in the file.
    fn entry_text(&self, client: AgentClient) -> String {
        let command = self.command.to_string_lossy();
        match client {
            AgentClient::ClaudeCode | AgentClient::Cursor => {
                json_edit::set("", &JSON_PATH, &self.entry()).unwrap_or_default()
            }
            AgentClient::Codex => format!(
                "[mcp_servers.localhost]\ncommand = {}\nargs = {}\n",
                toml_edit::render_string(&command),
                toml_edit::render_strings(ARGUMENTS)
            ),
        }
        .trim_end()
        .to_string()
    }

    /// The agent's `localhost` server as configured now.
    fn read_entry(&self, client: AgentClient) -> Result<Option<Entry>, String> {
        let path = client.config_path(&self.home);
        let text = match files::read(&path) {
            Ok(Some(text)) => text,
            Ok(None) => return Ok(None),
            Err(error) => return Err(error.message()),
        };
        match client {
            AgentClient::Codex => Ok(toml_edit::table(&text, &TOML_PATH)?
                .map(|table| Entry::new(table.string("command"), table.strings("args").unwrap_or_default()))),
            AgentClient::ClaudeCode | AgentClient::Cursor => Ok(json_edit::get(&text, &JSON_PATH)?.map(|entry| {
                let arguments = entry.get("args").and_then(|args| serde_json::from_value(args.clone()).ok());
                Entry::new(
                    entry.get("command").and_then(Value::as_str).map(str::to_string),
                    arguments.unwrap_or_default(),
                )
            })),
        }
    }
}

/// The `localhost` server of Claude Code and Cursor, fields in this order.
#[derive(Serialize)]
struct ServerEntry {
    #[serde(rename = "type")]
    kind: &'static str,
    command: String,
    args: &'static [&'static str],
}

struct Entry {
    command: Option<String>,
    has_mcp_arguments: bool,
    /// It runs `localhost mcp --read-only`.
    is_read_only: bool,
}

impl Entry {
    fn new(command: Option<String>, arguments: Vec<String>) -> Entry {
        let is_read_only = arguments == READ_ONLY_ARGUMENTS;
        Entry { command, has_mcp_arguments: is_read_only || arguments == ARGUMENTS, is_read_only }
    }
}

/// What an update asks before it restarts the app: it stops the active
/// Services and disconnects the coding agents connected through
/// `localhost mcp`, never without the user's yes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct UpdateConfirmation {
    pub title: String,
    pub button: String,
    /// What happens to the agents, when any is connected.
    pub agents_note: Option<String>,
}

impl UpdateConfirmation {
    /// `None` when nothing is running that the update would end.
    pub fn new(services: usize, agents: usize) -> Option<UpdateConfirmation> {
        let plural = |count: usize, one: &str, many: &str| format!("{count} {}", if count == 1 { one } else { many });
        let title = match (services, agents) {
            (0, 0) => return None,
            (services, 0) => format!("Stop {} and update?", plural(services, "service", "services")),
            (0, agents) => format!("Disconnect {} and update?", plural(agents, "agent", "agents")),
            (services, agents) => format!(
                "Stop {} and disconnect {} and update?",
                plural(services, "service", "services"),
                plural(agents, "agent", "agents")
            ),
        };
        let agents_note = (agents > 0).then(|| {
            if agents == 1 {
                "1 coding agent is connected through `localhost mcp`. It is disconnected so the update can replace the localhost command: start a new session in it afterwards.".to_string()
            } else {
                format!("{agents} coding agents are connected through `localhost mcp`. They are disconnected so the update can replace the localhost command: start a new session in each afterwards.")
            }
        });
        let button = if services > 0 { "Stop and Update" } else { "Disconnect and Update" };
        Some(UpdateConfirmation { title, button: button.into(), agents_note })
    }
}

/// Runs `claude` with `arguments`, with the terminal's `PATH` (an npm
/// install needs `node`), for up to a minute. Fails with what it printed.
fn run_claude(claude: &Path, arguments: &[&str], path: Option<String>) -> Result<(), LocalhostError> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let shown = format!("claude {}", arguments[..arguments.len().min(5)].join(" "));
    let failed = |output: String| LocalhostError::AgentCommandFailed { command: shown.clone(), output };
    let mut command = Command::new(claude);
    command.args(arguments).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(path) = path {
        command.env("PATH", path);
    }
    let mut child = command.spawn().map_err(|error| failed(error.to_string()))?;
    let started = Instant::now();
    while child.try_wait().map_err(|error| failed(error.to_string()))?.is_none() {
        if started.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(failed("it didn’t finish within a minute".into()));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output().map_err(|error| failed(error.to_string()))?;
    if output.status.success() {
        return Ok(());
    }
    let printed = format!("{}{}", String::from_utf8_lossy(&output.stderr), String::from_utf8_lossy(&output.stdout));
    let printed = printed.trim();
    let tail: String = printed.chars().rev().take(400).collect::<Vec<_>>().into_iter().rev().collect();
    Err(failed(if tail.is_empty() { format!("it exited with {}", output.status) } else { tail }))
}

/// Whether a SKILL.md is localhost's: its front matter names it localhost-mcp.
fn is_our_skill(text: &str) -> bool {
    text.strip_prefix("---")
        .and_then(|rest| rest.split("\n---").next())
        .is_some_and(|front| front.lines().any(|line| line.trim() == format!("name: {SKILL_FOLDER}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_update_asks_about_services_and_agents() {
        assert_eq!(UpdateConfirmation::new(0, 0), None);
        let services = UpdateConfirmation::new(2, 0).unwrap();
        assert_eq!(
            (services.title.as_str(), services.button.as_str()),
            ("Stop 2 services and update?", "Stop and Update")
        );
        assert_eq!(services.agents_note, None);
        let agents = UpdateConfirmation::new(0, 1).unwrap();
        assert_eq!(
            (agents.title.as_str(), agents.button.as_str()),
            ("Disconnect 1 agent and update?", "Disconnect and Update")
        );
        assert!(agents.agents_note.unwrap().contains("start a new session in it"));
        assert_eq!(UpdateConfirmation::new(1, 3).unwrap().title, "Stop 1 service and disconnect 3 agents and update?");
    }

    #[test]
    fn the_bundled_skill_is_recognised_as_ours() {
        assert!(is_our_skill(AGENT_SKILL));
        assert!(!is_our_skill("---\nname: other\n---\nname: localhost-mcp\n"));
    }
}
