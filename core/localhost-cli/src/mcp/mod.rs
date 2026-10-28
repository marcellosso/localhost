//! `localhost mcp`: a Model Context Protocol server on standard input and
//! output, so a coding agent starts, stops and inspects Services through
//! localhost instead of a shell the user can't see.
//!
//! The protocol is JSON-RPC 2.0, one message per line. Only what tools need
//! is implemented: `initialize`, `ping`, `tools/list` and `tools/call`.
//! Requests are handled concurrently, so a status can be read while a start
//! waits for its Service to come up.
//!
//! Services run in this process's own ServiceManager, recorded like any
//! other session's Runs. When the agent disconnects (standard input closes)
//! or the process is told to end, every Run it started is stopped first.
//! Standard output carries the protocol only.

mod tools;

use std::process::ExitCode;
use std::sync::Arc;

use localhost_core::{
    LogStore, Origin, ProjectLibrary, RunRegistry, ServiceManager, ServiceManagerOptions, default_storage_root,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use self::tools::Tools;

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
Services you start here stop when this connection ends.";

pub fn run(library: ProjectLibrary) -> Result<ExitCode, String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("Couldn't start: {error}"))?;
    runtime.block_on(serve(library))?;
    Ok(ExitCode::SUCCESS)
}

async fn serve(library: ProjectLibrary) -> Result<(), String> {
    let storage_root = default_storage_root();
    let library = Arc::new(library);
    let logs = Arc::new(LogStore::new(&storage_root));
    let registry = Arc::new(RunRegistry::new(&storage_root));
    let manager = ServiceManager::new(library.clone(), logs.clone(), registry, ServiceManagerOptions::default());
    let tools = Arc::new(Tools::new(library, manager.clone(), logs, storage_root));

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

    let mut terminate = signal(SignalKind::terminate()).map_err(|error| error.to_string())?;
    let mut interrupt = signal(SignalKind::interrupt()).map_err(|error| error.to_string())?;
    let mut hang_up = signal(SignalKind::hangup()).map_err(|error| error.to_string())?;
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
            _ = terminate.recv() => break,
            _ = interrupt.recv() => break,
            _ = hang_up.recv() => break,
            Some(_) = requests.join_next(), if !requests.is_empty() => {}
        }
    }

    manager.stop_every_run(Origin::Agent).await;
    // A start that was launching when the stop began may have launched since.
    requests.abort_all();
    while requests.join_next().await.is_some() {}
    manager.stop_every_run(Origin::Agent).await;
    drop(replies);
    let _ = writer.await;
    Ok(())
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
