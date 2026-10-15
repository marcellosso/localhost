//! `localhostd --stdio`: the core, driven by the Windows app over this
//! process's standard input and output (`localhost_core::remote`). The app
//! starts it inside a WSL distro, one per distro in use (ADR-0008).
//!
//! Standard output carries the protocol only; diagnostics go to standard
//! error. When standard input closes, every Run is stopped and it exits.

#[cfg(unix)]
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: localhostd --stdio | --version";

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["--stdio"] => serve(),
        ["--version"] => {
            println!("localhostd {}", localhost_core::remote::Hello::current().version);
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Where Runs' logs and records are kept: a folder of their own, so they
/// never mix with those of a localhost app running in the same distro.
/// `LOCALHOST_STORAGE_ROOT` overrides it.
#[cfg(unix)]
fn storage_root() -> PathBuf {
    match std::env::var_os("LOCALHOST_STORAGE_ROOT") {
        Some(root) => root.into(),
        None => localhost_core::default_storage_root().join("localhostd"),
    }
}

#[cfg(unix)]
fn serve() -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("localhostd: {error}");
            return ExitCode::FAILURE;
        }
    };
    let root = storage_root();
    runtime.block_on(localhost_core::remote::serve(
        tokio::io::stdin(),
        tokio::io::stdout(),
        &root,
        localhost_core::ServiceManagerOptions::default(),
    ));
    ExitCode::SUCCESS
}

#[cfg(not(unix))]
fn serve() -> ExitCode {
    eprintln!("localhostd runs inside a WSL distro, not on Windows itself.");
    ExitCode::FAILURE
}
