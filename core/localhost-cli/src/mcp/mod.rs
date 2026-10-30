//! `localhost mcp`: a Model Context Protocol server on standard input and
//! output, so a coding agent starts, stops and inspects Services through
//! localhost instead of a shell the user can't see.
//!
//! The protocol is JSON-RPC 2.0, one message per line. Only what tools need
//! is implemented: `initialize`, `ping`, `tools/list` and `tools/call`.
//! Requests are handled concurrently, so a status can be read while a start
//! waits for its Service to come up.
//!
//! Services run in the shared core on this machine (`remote::shared`),
//! started if it isn't running yet, the one the app drives too: what the
//! agent starts is Running in the app, and the reverse. When the agent
//! disconnects (standard input closes) or the process is told to end, the
//! core stops the Runs it started, unless the app is open, or another
//! connected client needs them too. Standard output carries the protocol only.
//!
//! Before an update, the app may dismiss connected agents, after asking the
//! user: the server then ends, so the update can replace this command and
//! the agent's next session starts the new version.

mod tools;

use std::process::ExitCode;
use std::sync::Arc;

use std::path::Path;

use localhost_core::remote::{Client, shared};
use localhost_core::{
    ExecutionTarget, Launcher, Link, LogStore, ProjectLibrary, ServiceManagerOptions, Targets, default_storage_root,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use self::tools::Tools;
use crate::signals::Signals;

/// The protocol versions this server speaks, newest first. A client asking
/// for another gets the newest, and decides whether to go on.
const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// What the agent is told once connected, before it reads any tool.
const INSTRUCTIONS: &str = "\
localhost runs this developer's local Projects and their Services (dev servers, databases, workers). \
Start, stop and inspect them with these tools instead of running commands such as `pnpm dev` in a shell: \
the developer sees and controls what you start. \
Call list_projects first; name Projects and Services by Key. \
A Service is only running when status says `running`; check it before saying so. \
When a start fails or a Port is taken, read logs and ports before trying again. \
The developer's localhost app shows the Services you start, and you see those it started. \
Services you start here stop when this connection ends, unless the localhost app is open.";

pub fn run(library: ProjectLibrary) -> Result<ExitCode, String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("Couldn't start: {error}"))?;
    let served = runtime.block_on(serve(library));
    // Standard input is read on a blocking thread, which waits for input
    // forever when the server ended without it closing (dismissed, or a
    // signal): the runtime mustn't wait for it.
    runtime.shutdown_timeout(std::time::Duration::from_secs(1));
    served?;
    Ok(ExitCode::SUCCESS)
}

async fn serve(library: ProjectLibrary) -> Result<(), String> {
    let storage_root = default_storage_root();
    let library = Arc::new(library);
    let logs = Arc::new(LogStore::new(&storage_root));
    let targets =
        Targets::open(library.clone(), &storage_root, ServiceManagerOptions::default(), launcher(&storage_root));
    // Now, so the first tool call doesn't wait for the core to start. If it
    // can't, each call that needs it says why.
    let _ = targets.connect(&ExecutionTarget::ThisMachine).await;
    let mut dismissed = {
        let targets = targets.clone();
        tokio::spawn(async move { targets.dismissed().await })
    };
    let tools = Arc::new(Tools::new(library, targets, logs, storage_root));

    let (replies, mut outgoing) = mpsc::unbounded_channel::<Value>();
    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(message) = outgoing.recv().await {
            let mut line = message.to_string();
            line.push('\n');
            if stdout.write_all(line.as_bytes()).await.is_err() || stdout.flush().await.is_err() {
                return;
            }
        }
    });

    let mut signals = Signals::new().map_err(|error| error.to_string())?;
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut requests = JoinSet::new();
    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Ok(Some(line)) = line else { break };
                if line.trim().is_empty() {
                    continue;
                }
                let tools = tools.clone();
                let replies = replies.clone();
                requests.spawn(async move {
                    if let Some(reply) = handle(&tools, &line).await {
                        let _ = replies.send(reply);
                    }
                });
            }
            _ = signals.next() => break,
            _ = &mut dismissed => {
                eprintln!("localhost: disconnected by the localhost app to install an update. Start a new session to reconnect.");
                break;
            }
            Some(_) = requests.join_next(), if !requests.is_empty() => {}
        }
    }
    dismissed.abort();

    // Closing the connection tells the core to stop what only this agent needed.
    requests.abort_all();
    while requests.join_next().await.is_some() {}
    drop(tools);
    drop(replies);
    let _ = writer.await;
    Ok(())
}

/// This machine through the shared core, started as `localhost daemon` if
/// it isn't running. A WSL distro's Projects are run by the Windows app only.
fn launcher(storage_root: &Path) -> Launcher {
    let storage_root = storage_root.to_path_buf();
    Box::new(move |target| match target {
        ExecutionTarget::ThisMachine => {
            let executable = std::env::current_exe().unwrap_or_else(|_| "localhost".into());
            let mut daemon = std::process::Command::new(executable);
            daemon.arg("daemon").arg(&storage_root);
            shared::link(&storage_root, Client::Agent, daemon)
        }
        ExecutionTarget::Wsl { distro } => {
            Link::Unavailable(format!("“{distro}” is reached only from the Windows app"))
        }
    })
}

/// The reply to one message, or `None` for a notification.
async fn handle(tools: &Tools, line: &str) -> Option<Value> {
    let message: Value = match serde_json::from_str(line) {
        Ok(message) => message,
        Err(error) => return Some(failure(Value::Null, -32700, &format!("Parse error: {error}"))),
    };
    let id = message.get("id").cloned();
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        // A reply to a request this server never sends, or not JSON-RPC at all.
        return id.map(|id| failure(id, -32600, "Invalid request: no method."));
    };
    // Notifications (`notifications/initialized`, `notifications/cancelled`) need no reply.
    let id = id?;
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => Ok(initialize(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools::definitions() })),
        "tools/call" => match params.get("name").and_then(Value::as_str) {
            Some(name) => Ok(tools.call(name, params.get("arguments").unwrap_or(&Value::Null)).await),
            None => Err((-32602, "Invalid params: tools/call needs a tool name.".to_string())),
        },
        _ => Err((-32601, format!("Method not found: {method}"))),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => failure(id, code, &message),
    })
}

fn initialize(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let version = PROTOCOL_VERSIONS.into_iter().find(|version| Some(*version) == requested);
    json!({
        "protocolVersion": version.unwrap_or(PROTOCOL_VERSIONS[0]),
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "localhost", "title": "localhost", "version": localhost_core::VERSION },
        "instructions": INSTRUCTIONS,
    })
}

fn failure(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_agrees_on_a_version_it_speaks() {
        let reply = initialize(&json!({ "protocolVersion": "2025-03-26" }));
        assert_eq!(reply["protocolVersion"], "2025-03-26");
        assert_eq!(reply["serverInfo"]["name"], "localhost");
        assert!(reply["capabilities"]["tools"].is_object());

        let reply = initialize(&json!({ "protocolVersion": "1999-01-01" }));
        assert_eq!(reply["protocolVersion"], PROTOCOL_VERSIONS[0]);
    }
}
