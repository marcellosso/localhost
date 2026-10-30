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
    /// Where Connect installs the localhost-mcp skill.
    pub skill_path: String,
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
}

impl AgentSetup {
    /// Agents of this user, pointed at `command`; first-edit backups go in
    /// `storage_root`'s `agent-backups` folder.
    pub fn new(command: impl Into<PathBuf>, storage_root: &Path) -> Self {
        Self::with_home(crate::paths::home_directory(), command, storage_root)
    }

    /// As `new`, for agents configured under `home` instead of the user's home folder.
    pub fn with_home(home: impl Into<PathBuf>, command: impl Into<PathBuf>, storage_root: &Path) -> Self {
        Self {
            home: home.into(),
            command: command.into(),
            backups: storage_root.join("agent-backups"),
            terminal_path: OnceLock::new(),
        }
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
            skill_path: skill_path.display().to_string(),
            configured_command: None,
            is_skill_installed: files::read(&skill_path).ok().flatten().is_some_and(|text| is_our_skill(&text)),
            backup_path: exists(&backup).then(|| backup.display().to_string()),
        };
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
        let is_current_skill = files::read(&skill_path).ok().flatten().is_some_and(|text| text == AGENT_SKILL);
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
        let edited = match client {
            AgentClient::Codex => toml_edit::set(
                text,
                &TOML_PATH,
                &[("command", toml_edit::render_string(&command)), ("args", toml_edit::render_strings(&["mcp"]))],
            ),
            AgentClient::ClaudeCode | AgentClient::Cursor => {
                match json_edit::get(text, &JSON_PATH) {
                    // A stdio server of the user's: only what runs it changes, its `env` and the rest stay.
                    Ok(Some(Value::Object(entry)))
                        if !entry.contains_key("url") && entry.get("type").is_none_or(|kind| kind == "stdio") =>
                    {
                        json_edit::set(text, &[JSON_PATH[0], SERVER, "command"], &json!(command))
                            .and_then(|text| json_edit::set(&text, &[JSON_PATH[0], SERVER, "args"], &json!(["mcp"])))
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
        let skill = client.skill_path(&self.home);
        if files::read(&skill)?.as_deref() != Some(AGENT_SKILL) {
            files::write(&skill, AGENT_SKILL)?;
        }
        Ok(self.status(client))
    }

    /// Removes the `localhost` server from the agent's configuration, and
    /// the skill if it is localhost's. Everything else stays.
    pub fn disconnect(&self, client: AgentClient) -> Result<AgentStatus, LocalhostError> {
        let path = client.config_path(&self.home);
        if let Some(text) = files::read(&path)? {
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
        Ok(self.status(client))
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
        ServerEntry { kind: "stdio", command: self.command.to_string_lossy().into_owned(), args: ["mcp"] }
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
                toml_edit::render_strings(&["mcp"])
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
            AgentClient::Codex => Ok(toml_edit::table(&text, &TOML_PATH)?.map(|table| Entry {
                command: table.string("command"),
                has_mcp_arguments: table.strings("args").is_some_and(|args| args == ["mcp"]),
            })),
            AgentClient::ClaudeCode | AgentClient::Cursor => {
                Ok(json_edit::get(&text, &JSON_PATH)?.map(|entry| Entry {
                    command: entry.get("command").and_then(Value::as_str).map(str::to_string),
                    has_mcp_arguments: entry.get("args") == Some(&json!(["mcp"])),
                }))
            }
        }
    }
}

/// The `localhost` server of Claude Code and Cursor, fields in this order.
#[derive(Serialize)]
struct ServerEntry {
    #[serde(rename = "type")]
    kind: &'static str,
    command: String,
    args: [&'static str; 1],
}

struct Entry {
    command: Option<String>,
    has_mcp_arguments: bool,
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
    fn the_bundled_skill_is_recognised_as_ours() {
        assert!(is_our_skill(AGENT_SKILL));
        assert!(!is_our_skill("---\nname: other\n---\nname: localhost-mcp\n"));
    }
}
