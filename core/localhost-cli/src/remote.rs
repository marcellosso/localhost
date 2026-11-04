//! `localhost remote --stdio`: this Machine's Projects and Services, served
//! to the app of another Machine, which runs this command over SSH
//! (`localhost_core::machine`). Standard output carries the protocol only.
//!
//! It is a client of this Machine's shared core, as `localhost mcp` is, so
//! the other Machine sees what agents here start, and they see what it
//! does. When the connection ends (standard input closes, SSH hangs up, or
//! the app goes silent) the core stops what only it started, unless an app
//! here is open.

use std::process::ExitCode;
use std::sync::Arc;

use localhost_core::machine::relay;
use localhost_core::remote::Client;
use localhost_core::{ExecutionTarget, ProjectLibrary, ServiceManagerOptions, Targets, default_storage_root};

use crate::signals::Signals;

pub fn run(library: ProjectLibrary) -> Result<ExitCode, String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("Couldn't start: {error}"))?;
    runtime.block_on(async {
        let storage_root = default_storage_root();
        let library = Arc::new(library);
        let targets = Targets::open(
            library.clone(),
            &storage_root,
            ServiceManagerOptions::default(),
            crate::mcp::launcher(&storage_root, Client::Machine),
        );
        // Now, so the statuses the app subscribes to are this Machine's.
        targets.connect(&ExecutionTarget::ThisMachine).await.map_err(|error| error.message())?;
        let mut signals = Signals::new().map_err(|error| error.to_string())?;
        tokio::select! {
            _ = relay::serve(tokio::io::stdin(), tokio::io::stdout(), library, targets, relay::SILENCE_TIMEOUT) => {}
            _ = signals.next() => {}
        }
        Ok::<(), String>(())
    })?;
    // Standard input is read on a blocking thread, which may wait forever.
    runtime.shutdown_timeout(std::time::Duration::from_secs(1));
    Ok(ExitCode::SUCCESS)
}
