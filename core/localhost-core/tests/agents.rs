//! Connecting coding agents, on sample configuration files in a temporary
//! home folder: never the real ones.

use std::fs;
use std::path::PathBuf;

use localhost_core::{AGENT_SKILL, AgentClient, AgentConnection, AgentSetup, LocalhostError};
use tempfile::TempDir;

const CLAUDE_JSON: &str = r#"{
  "numStartups": 42,
  "installMethod": "native",
  "projects": {
    "/Users/me/shop": {
      "allowedTools": [],
      "mcpServers": {
        "local-only": { "type": "stdio", "command": "node", "args": ["server.js"], "env": {} }
      }
    }
  },
  "mcpServers": {
    "github": {
      "type": "http",
      "url": "https://api.githubcopilot.com/mcp/"
    }
  },
  "tipsHistory": { "new-user-warmup": 7 },
  "lastReleaseNotesSeen": "2.1.0"
}
"#;

const CURSOR_JSON: &str = r#"{
  // Cursor accepts comments.
  "mcpServers": {
    "figma": { "url": "http://127.0.0.1:3845/sse" },
  },
}
"#;

const CODEX_TOML: &str = r#"model = "gpt-5-codex"
approval_policy = "on-request"

[mcp_servers.docs]
command = "npx"
args = ["-y", "docs-mcp"]

[mcp_servers.docs.env]
TOKEN = "x"

[profiles.fast]
model = "o4-mini"
"#;

struct Fixture {
    _temporary: TempDir,
    home: PathBuf,
    storage: PathBuf,
    command: PathBuf,
}

impl Fixture {
    /// A home folder where all three agents are installed, with configuration of their own.
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let home = temporary.path().join("home");
        let storage = temporary.path().join("storage");
        let command = temporary.path().join("localhost.app/Contents/Helpers/localhost");
        fs::create_dir_all(command.parent().unwrap()).unwrap();
        fs::write(&command, "#!/bin/sh\n").unwrap();
        for folder in [".claude", ".cursor", ".codex"] {
            fs::create_dir_all(home.join(folder)).unwrap();
        }
        fs::write(home.join(".claude.json"), CLAUDE_JSON).unwrap();
        fs::write(home.join(".cursor/mcp.json"), CURSOR_JSON).unwrap();
        fs::write(home.join(".codex/config.toml"), CODEX_TOML).unwrap();
        Self { _temporary: temporary, home, storage, command }
    }

    fn setup(&self) -> AgentSetup {
        AgentSetup::with_home(&self.home, &self.command, &self.storage).with_terminal_path(Some("/usr/bin:/bin".into()))
    }

    fn read(&self, path: &str) -> String {
        fs::read_to_string(self.home.join(path)).unwrap()
    }

    fn command(&self) -> String {
        self.command.to_string_lossy().into_owned()
    }
}

fn config(client: AgentClient) -> &'static str {
    match client {
        AgentClient::ClaudeCode => ".claude.json",
        AgentClient::Cursor => ".cursor/mcp.json",
        AgentClient::Codex => ".codex/config.toml",
    }
}

fn original(client: AgentClient) -> &'static str {
    match client {
        AgentClient::ClaudeCode => CLAUDE_JSON,
        AgentClient::Cursor => CURSOR_JSON,
        AgentClient::Codex => CODEX_TOML,
    }
}

fn skill(client: AgentClient) -> &'static str {
    match client {
        AgentClient::ClaudeCode => ".claude/skills/localhost-mcp/SKILL.md",
        AgentClient::Cursor => ".cursor/skills/localhost-mcp/SKILL.md",
        AgentClient::Codex => ".agents/skills/localhost-mcp/SKILL.md",
    }
}

#[test]
fn connecting_adds_only_localhost_and_disconnecting_restores_every_byte() {
    let fixture = Fixture::new();
    let setup = fixture.setup();
    for client in AgentClient::ALL {
        assert_eq!(setup.status(client).connection, AgentConnection::NotConnected, "{client:?}");

        let status = setup.connect(client).unwrap();
        assert_eq!(status.connection, AgentConnection::Connected, "{client:?}: {}", status.summary);
        assert!(status.is_skill_installed);
        assert_eq!(status.configured_command.as_deref(), Some(fixture.command().as_str()));
        let connected = fixture.read(config(client));
        // As JSON and TOML strings write it: Windows' backslashes doubled.
        assert!(connected.contains(&fixture.command().replace('\\', "\\\\")), "{client:?}: {connected}");
        assert_eq!(fixture.read(skill(client)), AGENT_SKILL);

        // The first edit's backup is the file as it was.
        let backup = status.backup_path.expect("a backup");
        assert_eq!(fs::read_to_string(&backup).unwrap(), original(client));

        // Again: nothing changes.
        assert_eq!(setup.connect(client).unwrap().connection, AgentConnection::Connected);
        assert_eq!(fixture.read(config(client)), connected);

        let status = setup.disconnect(client).unwrap();
        assert_eq!(status.connection, AgentConnection::NotConnected);
        assert!(!status.is_skill_installed);
        assert_eq!(fixture.read(config(client)), original(client), "{client:?}");
        assert!(!fixture.home.join(skill(client)).parent().unwrap().exists());
        // Still the original, not the connected file.
        assert_eq!(fs::read_to_string(&backup).unwrap(), original(client));
    }
}

#[test]
fn the_entries_are_what_the_agents_document() {
    let fixture = Fixture::new();
    let setup = fixture.setup();
    for client in AgentClient::ALL {
        setup.connect(client).unwrap();
    }
    let command = fixture.command();

    let claude: serde_json::Value = serde_json::from_str(&fixture.read(".claude.json")).unwrap();
    assert_eq!(
        claude["mcpServers"]["localhost"],
        serde_json::json!({ "type": "stdio", "command": command, "args": ["mcp"] })
    );
    assert_eq!(claude["mcpServers"]["github"]["url"], "https://api.githubcopilot.com/mcp/");
    assert_eq!(claude["projects"]["/Users/me/shop"]["mcpServers"]["local-only"]["command"], "node");
    assert_eq!(claude["numStartups"], 42);

    let cursor = fixture.read(".cursor/mcp.json");
    assert!(cursor.starts_with("{\n  // Cursor accepts comments.\n  \"mcpServers\": {\n    \"figma\""));
    assert!(cursor.contains(&format!(
        "\"localhost\": {{\n      \"type\": \"stdio\",\n      \"command\": {},",
        serde_json::json!(command)
    )));

    let codex = fixture.read(".codex/config.toml");
    assert!(codex.starts_with(CODEX_TOML));
    assert!(codex.ends_with(&format!(
        "\n[mcp_servers.localhost]\ncommand = {}\nargs = [\"mcp\"]\n",
        serde_json::json!(command)
    )));

    // What the screen shows is what was written.
    assert!(codex.contains(&setup.status(AgentClient::Codex).entry));
    let shown: serde_json::Value = serde_json::from_str(&setup.status(AgentClient::Cursor).entry).unwrap();
    assert_eq!(shown["mcpServers"]["localhost"], claude["mcpServers"]["localhost"]);
}

#[test]
fn an_entry_running_another_localhost_is_flagged_and_repointed_keeping_the_rest() {
    let fixture = Fixture::new();
    let cargo = fixture.home.join(".cargo/bin/localhost");
    fs::create_dir_all(cargo.parent().unwrap()).unwrap();
    fs::write(&cargo, "").unwrap();
    let cargo = cargo.to_string_lossy().into_owned();
    fs::write(
        fixture.home.join(".claude.json"),
        format!(
            "{{\n  \"mcpServers\": {{\n    \"localhost\": {{\n      \"type\": \"stdio\",\n      \"command\": {},\n      \"args\": [\"mcp\"],\n      \"env\": {{ \"RUST_LOG\": \"debug\" }}\n    }}\n  }}\n}}\n",
            serde_json::json!(cargo)
        ),
    )
    .unwrap();
    fs::write(
        fixture.home.join(".codex/config.toml"),
        "[mcp_servers.localhost]\ncommand = \"localhost\"\nargs = [\"mcp\"]\nstartup_timeout_sec = 30\n",
    )
    .unwrap();
    let setup = fixture.setup();

    let claude = setup.status(AgentClient::ClaudeCode);
    assert_eq!(claude.connection, AgentConnection::NeedsUpdate);
    assert_eq!(claude.configured_command.as_deref(), Some(cargo.as_str()));
    assert!(claude.summary.contains("another localhost command"), "{}", claude.summary);
    let codex = setup.status(AgentClient::Codex);
    assert_eq!(codex.connection, AgentConnection::NeedsUpdate);
    assert_eq!(codex.configured_command.as_deref(), Some("localhost"));

    assert_eq!(setup.connect(AgentClient::ClaudeCode).unwrap().connection, AgentConnection::Connected);
    let claude: serde_json::Value = serde_json::from_str(&fixture.read(".claude.json")).unwrap();
    assert_eq!(claude["mcpServers"]["localhost"]["command"], fixture.command());
    assert_eq!(claude["mcpServers"]["localhost"]["env"]["RUST_LOG"], "debug");

    assert_eq!(setup.connect(AgentClient::Codex).unwrap().connection, AgentConnection::Connected);
    assert_eq!(
        fixture.read(".codex/config.toml"),
        format!(
            "[mcp_servers.localhost]\ncommand = {}\nargs = [\"mcp\"]\nstartup_timeout_sec = 30\n",
            serde_json::json!(fixture.command())
        )
    );
}

#[test]
fn a_missing_command_or_an_old_skill_needs_an_update() {
    let fixture = Fixture::new();
    let setup = fixture.setup();
    setup.connect(AgentClient::Cursor).unwrap();

    fs::write(fixture.home.join(skill(AgentClient::Cursor)), "---\nname: localhost-mcp\n---\nOlder.\n").unwrap();
    let status = setup.status(AgentClient::Cursor);
    assert_eq!(status.connection, AgentConnection::NeedsUpdate);
    assert!(status.summary.contains("skill"), "{}", status.summary);
    setup.connect(AgentClient::Cursor).unwrap();
    assert_eq!(fixture.read(skill(AgentClient::Cursor)), AGENT_SKILL);

    // The app moved: the configured command is gone.
    let moved = AgentSetup::with_home(&fixture.home, fixture.home.join("elsewhere/localhost"), &fixture.storage);
    let status = moved.status(AgentClient::Cursor);
    assert_eq!(status.connection, AgentConnection::NeedsUpdate);
    fs::remove_file(&fixture.command).unwrap();
    let status = setup.status(AgentClient::Cursor);
    assert_eq!(status.connection, AgentConnection::Connected, "the same path, so nothing to repoint");
    let status = moved.status(AgentClient::Cursor);
    assert!(status.summary.contains("no longer exists"), "{}", status.summary);
}

#[test]
fn disconnecting_leaves_a_skill_that_isnt_localhosts() {
    let fixture = Fixture::new();
    let setup = fixture.setup();
    let path = fixture.home.join(skill(AgentClient::Codex));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "---\nname: my-own\n---\n").unwrap();
    setup.disconnect(AgentClient::Codex).unwrap();
    assert!(path.exists());
    assert_eq!(fixture.read(config(AgentClient::Codex)), CODEX_TOML);
    assert!(setup.status(AgentClient::Codex).backup_path.is_none(), "nothing was edited");
}

#[test]
fn missing_agents_and_files() {
    let fixture = Fixture::new();
    fs::remove_dir_all(fixture.home.join(".cursor")).unwrap();
    fs::remove_file(fixture.home.join(".codex/config.toml")).unwrap();
    let setup = fixture.setup();

    let cursor = setup.status(AgentClient::Cursor);
    assert_eq!(cursor.connection, AgentConnection::NotInstalled);
    assert!(matches!(setup.connect(AgentClient::Cursor), Err(LocalhostError::AgentNotInstalled(_))));
    assert!(!fixture.home.join(".cursor").exists());

    // Codex is installed without a config.toml yet: Connect creates it.
    assert_eq!(setup.status(AgentClient::Codex).connection, AgentConnection::NotConnected);
    setup.connect(AgentClient::Codex).unwrap();
    assert!(fixture.read(".codex/config.toml").starts_with("[mcp_servers.localhost]\n"));
    assert!(setup.status(AgentClient::Codex).backup_path.is_none(), "there was nothing to back up");

    // Claude Code with only ~/.claude.json.
    fs::remove_dir_all(fixture.home.join(".claude")).unwrap();
    assert_eq!(setup.status(AgentClient::ClaudeCode).connection, AgentConnection::NotConnected);
}

#[test]
fn a_file_it_cant_edit_is_reported_and_left_alone() {
    let fixture = Fixture::new();
    fs::write(fixture.home.join(".cursor/mcp.json"), "{ \"mcpServers\": ").unwrap();
    fs::write(fixture.home.join(".codex/config.toml"), "[mcp_servers]\nlocalhost = { command = \"x\" }\n").unwrap();
    let setup = fixture.setup();
    for client in [AgentClient::Cursor, AgentClient::Codex] {
        let before = fixture.read(config(client));
        let status = setup.status(client);
        assert_eq!(status.connection, AgentConnection::Unreadable, "{client:?}");
        assert!(status.summary.contains("by hand"), "{}", status.summary);
        assert!(matches!(setup.connect(client), Err(LocalhostError::AgentConfigUnsupported { .. })));
        assert!(matches!(setup.disconnect(client), Err(LocalhostError::AgentConfigUnsupported { .. })));
        assert_eq!(fixture.read(config(client)), before);
        assert!(!fixture.home.join(skill(client)).exists());
    }
}

#[test]
fn an_app_without_its_command_connects_nothing() {
    let fixture = Fixture::new();
    let setup = AgentSetup::with_home(&fixture.home, fixture.home.join("target/debug/localhost"), &fixture.storage);
    assert!(matches!(setup.connect(AgentClient::ClaudeCode), Err(LocalhostError::CommandNotBundled(_))));
    assert_eq!(fixture.read(".claude.json"), CLAUDE_JSON);
}

#[cfg(unix)]
#[test]
fn a_symlinked_config_is_edited_where_it_points() {
    let fixture = Fixture::new();
    let dotfiles = fixture.home.join("dotfiles/claude.json");
    fs::create_dir_all(dotfiles.parent().unwrap()).unwrap();
    fs::rename(fixture.home.join(".claude.json"), &dotfiles).unwrap();
    std::os::unix::fs::symlink(&dotfiles, fixture.home.join(".claude.json")).unwrap();
    fixture.setup().connect(AgentClient::ClaudeCode).unwrap();
    assert!(fs::symlink_metadata(fixture.home.join(".claude.json")).unwrap().file_type().is_symlink());
    assert!(fs::read_to_string(&dotfiles).unwrap().contains("\"localhost\""));
}

#[cfg(unix)]
#[test]
fn the_command_is_linked_into_local_bin_and_unlinked() {
    use localhost_core::CommandLineState;

    let fixture = Fixture::new();
    let link = fixture.home.join(".local/bin/localhost");
    let setup = fixture.setup();
    assert_eq!(setup.command_line().state, CommandLineState::NotAdded);

    let status = setup.add_command_to_path().unwrap();
    assert_eq!(status.state, CommandLineState::Added);
    assert_eq!(fs::read_link(&link).unwrap(), fixture.command);
    assert!(status.summary.contains("isn’t on your PATH"), "{}", status.summary);
    let on_path = AgentSetup::with_home(&fixture.home, &fixture.command, &fixture.storage)
        .with_terminal_path(Some(format!("/usr/bin:{}", link.parent().unwrap().display())));
    assert_eq!(on_path.command_line().summary, "New terminals run `localhost`.");

    assert_eq!(setup.remove_command_from_path().unwrap().state, CommandLineState::NotAdded);
    assert!(fs::symlink_metadata(&link).is_err());

    // Someone else's file stays.
    fs::write(&link, "mine").unwrap();
    assert_eq!(setup.command_line().state, CommandLineState::Taken);
    assert!(matches!(setup.add_command_to_path(), Err(LocalhostError::CommandPathTaken(_))));
    setup.remove_command_from_path().unwrap();
    assert_eq!(fs::read_to_string(&link).unwrap(), "mine");

    // A package put it on the PATH already.
    let packaged = AgentSetup::with_home(&fixture.home, &fixture.command, &fixture.storage)
        .with_terminal_path(Some(fixture.command.parent().unwrap().display().to_string()));
    assert_eq!(packaged.command_line().state, CommandLineState::AlreadyOnPath);
}

#[test]
fn cursor_reads_the_skill_another_agent_has_instead_of_a_second_copy() {
    let fixture = Fixture::new();
    let setup = fixture.setup();
    let cursor_skill = fixture.home.join(skill(AgentClient::Cursor));

    // Cursor first: its own copy, until Claude Code's folder has one Cursor reads.
    setup.connect(AgentClient::Cursor).unwrap();
    assert!(cursor_skill.exists());
    setup.connect(AgentClient::ClaudeCode).unwrap();
    assert!(!cursor_skill.parent().unwrap().exists(), "one copy only");
    let cursor = setup.status(AgentClient::Cursor);
    assert_eq!(cursor.connection, AgentConnection::Connected, "{}", cursor.summary);
    assert_eq!(cursor.skill_shared_from.as_deref(), Some("Claude Code"));
    assert!(cursor.summary.contains("isn’t installed twice"), "{}", cursor.summary);

    // Connecting Cursor again doesn't add one.
    setup.connect(AgentClient::Cursor).unwrap();
    assert!(!cursor_skill.exists());

    // Claude Code leaves: Cursor gets its own again.
    setup.disconnect(AgentClient::ClaudeCode).unwrap();
    assert_eq!(fixture.read(skill(AgentClient::Cursor)), AGENT_SKILL);
    assert_eq!(setup.status(AgentClient::Cursor).connection, AgentConnection::Connected);

    // Codex's folder (~/.agents/skills) counts too; disconnecting Cursor never touches it.
    setup.connect(AgentClient::Codex).unwrap();
    assert!(!cursor_skill.exists());
    assert_eq!(setup.status(AgentClient::Cursor).skill_shared_from.as_deref(), Some("Codex"));
    setup.disconnect(AgentClient::Cursor).unwrap();
    assert_eq!(fixture.read(skill(AgentClient::Codex)), AGENT_SKILL);

    // A disconnected Cursor gets no copy when another agent leaves.
    setup.disconnect(AgentClient::Codex).unwrap();
    assert!(!cursor_skill.exists());
}

/// A stand-in for Claude Code's `claude`, which logs its arguments and
/// writes `~/.claude.json` the way `claude mcp add-json` / `remove` would.
#[cfg(unix)]
fn fake_claude(fixture: &Fixture) -> (PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;

    let script = fixture.home.join("bin/claude");
    let log = fixture.home.join("claude.log");
    fs::create_dir_all(script.parent().unwrap()).unwrap();
    let config = fixture.home.join(".claude.json");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{log}'\ncase \"$2\" in\n  add-json) printf '{{\"mcpServers\": {{\"%s\": %s}}}}\\n' \"$5\" \"$6\" > '{config}' ;;\n  remove) printf '{{}}\\n' > '{config}' ;;\nesac\n",
            log = log.display(),
            config = config.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    (script, log)
}

#[cfg(unix)]
#[test]
fn claude_code_is_changed_through_its_own_command_when_there_is_one() {
    let fixture = Fixture::new();
    let (claude, log) = fake_claude(&fixture);
    let setup = fixture.setup().with_claude_command(Some(claude));
    let lines = || fs::read_to_string(&log).unwrap_or_default().lines().map(str::to_string).collect::<Vec<_>>();

    assert_eq!(
        setup.status(AgentClient::ClaudeCode).via_command.as_deref(),
        Some("claude mcp add-json --scope user localhost")
    );
    let status = setup.connect(AgentClient::ClaudeCode).unwrap();
    assert_eq!(status.connection, AgentConnection::Connected, "{}", status.summary);
    let added = lines();
    assert_eq!(added.len(), 1, "{added:?}");
    assert!(added[0].starts_with("mcp add-json --scope user localhost {"), "{added:?}");
    assert!(added[0].contains(&fixture.command()), "{added:?}");
    // Backed up first, as when it edits the file itself.
    assert_eq!(fs::read_to_string(status.backup_path.unwrap()).unwrap(), CLAUDE_JSON);

    // Again: nothing to run.
    setup.connect(AgentClient::ClaudeCode).unwrap();
    assert_eq!(lines().len(), 1);

    // Repointing removes and adds it back, keeping its `env`.
    fs::write(
        fixture.home.join(".claude.json"),
        r#"{"mcpServers": {"localhost": {"type": "stdio", "command": "/old/localhost", "args": ["mcp"], "env": {"A": "1"}}}}"#,
    )
    .unwrap();
    assert_eq!(setup.status(AgentClient::ClaudeCode).connection, AgentConnection::NeedsUpdate);
    setup.connect(AgentClient::ClaudeCode).unwrap();
    let repointed = lines();
    assert_eq!(repointed[1], "mcp remove --scope user localhost");
    assert!(repointed[2].contains(r#""env":{"A":"1"}"#) && repointed[2].contains(&fixture.command()), "{repointed:?}");

    setup.disconnect(AgentClient::ClaudeCode).unwrap();
    assert_eq!(lines()[3], "mcp remove --scope user localhost");
    assert_eq!(setup.status(AgentClient::ClaudeCode).connection, AgentConnection::NotConnected);
    // Nothing left to remove: it isn't run again.
    setup.disconnect(AgentClient::ClaudeCode).unwrap();
    assert_eq!(lines().len(), 4);
}

#[cfg(unix)]
#[test]
fn a_failing_claude_command_is_reported_and_the_file_left_to_it() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new();
    let claude = fixture.home.join("bin/claude");
    fs::create_dir_all(claude.parent().unwrap()).unwrap();
    fs::write(&claude, "#!/bin/sh\necho 'MCP server localhost already exists' >&2\nexit 1\n").unwrap();
    fs::set_permissions(&claude, fs::Permissions::from_mode(0o755)).unwrap();
    let setup = fixture.setup().with_claude_command(Some(claude));
    match setup.connect(AgentClient::ClaudeCode) {
        Err(error @ LocalhostError::AgentCommandFailed { .. }) => {
            assert!(error.message().contains("already exists"), "{}", error.message())
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(fixture.read(".claude.json"), CLAUDE_JSON);
}

#[test]
fn without_claude_the_file_is_edited() {
    let fixture = Fixture::new();
    let setup = fixture.setup().with_claude_command(None);
    assert_eq!(setup.status(AgentClient::ClaudeCode).via_command, None);
    setup.connect(AgentClient::ClaudeCode).unwrap();
    assert!(fixture.read(".claude.json").contains("\"localhost\""));
}

#[test]
fn a_server_made_read_only_by_hand_is_connected_and_connect_keeps_it_read_only() {
    let fixture = Fixture::new();
    let setup = fixture.setup();
    let command = fixture.command();
    fs::write(
        fixture.home.join(".cursor/mcp.json"),
        format!(r#"{{"mcpServers": {{"localhost": {{"type": "stdio", "command": "{command}", "args": ["mcp", "--read-only"]}}}}}}"#),
    )
    .unwrap();
    fs::write(
        fixture.home.join(".codex/config.toml"),
        "[mcp_servers.localhost]\ncommand = \"/old/localhost\"\nargs = [\"mcp\", \"--read-only\"]\n",
    )
    .unwrap();

    setup.connect(AgentClient::Cursor).unwrap();
    let status = setup.status(AgentClient::Cursor);
    assert_eq!(status.connection, AgentConnection::Connected, "{}", status.summary);
    assert!(status.summary.contains("read-only"), "{}", status.summary);
    let cursor_args = || {
        let text = fs::read_to_string(fixture.home.join(".cursor/mcp.json")).unwrap();
        serde_json::from_str::<serde_json::Value>(&text).unwrap()["mcpServers"]["localhost"]["args"].clone()
    };
    assert_eq!(cursor_args(), serde_json::json!(["mcp", "--read-only"]));

    // Updating a server that runs another command keeps it read-only.
    assert_eq!(setup.status(AgentClient::Codex).connection, AgentConnection::NeedsUpdate);
    setup.connect(AgentClient::Codex).unwrap();
    let codex = fs::read_to_string(fixture.home.join(".codex/config.toml")).unwrap();
    assert!(codex.contains(r#"args = ["mcp", "--read-only"]"#), "{codex}");
    assert_eq!(setup.status(AgentClient::Codex).connection, AgentConnection::Connected);

    // Any other arguments still need an update, which makes them `mcp`.
    fs::write(
        fixture.home.join(".cursor/mcp.json"),
        format!(r#"{{"mcpServers": {{"localhost": {{"type": "stdio", "command": "{command}", "args": ["mcp", "--verbose"]}}}}}}"#),
    )
    .unwrap();
    assert_eq!(setup.status(AgentClient::Cursor).connection, AgentConnection::NeedsUpdate);
    setup.connect(AgentClient::Cursor).unwrap();
    assert_eq!(cursor_args(), serde_json::json!(["mcp"]));
}
