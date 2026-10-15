//! How the app reaches each Execution Target (`localhost_core::Targets`).
//! This machine runs in this process. A WSL distro is reached through the
//! `localhostd` inside it (ADR-0008), which only the Windows app starts.
//!
//! For development on Linux and macOS, `LOCALHOST_DEV_LOCALHOSTD=<path to
//! localhostd>` reaches every target, this machine included, through a
//! `localhostd` started here: one per target, as the Windows app runs one
//! per distro. So the Windows app's path is tested without Windows. Each
//! keeps its Runs' logs and records in `<storage root>/localhostd/<target>`.

use std::path::{Path, PathBuf};

use localhost_core::remote::Remote;
use localhost_core::{ExecutionTarget, Launcher, Link};

const DEV_LOCALHOSTD: &str = "LOCALHOST_DEV_LOCALHOSTD";

pub fn launcher(storage_root: &Path) -> Launcher {
    launcher_with(storage_root, std::env::var_os(DEV_LOCALHOSTD).map(PathBuf::from))
}

/// `localhostd`: the one to reach every target through, if any.
fn launcher_with(storage_root: &Path, localhostd: Option<PathBuf>) -> Launcher {
    let roots = storage_root.join("localhostd");
    Box::new(move |target| match (&localhostd, target) {
        (Some(localhostd), target) => {
            let mut command = tokio::process::Command::new(localhostd);
            command.arg("--stdio").env("LOCALHOST_STORAGE_ROOT", roots.join(folder_name(target)));
            Link::Remote(Box::pin(Remote::spawn(command)))
        }
        (None, ExecutionTarget::ThisMachine) => Link::InProcess,
        (None, ExecutionTarget::Wsl { distro }) => Link::Unavailable(if cfg!(windows) {
            format!("this build can’t start localhost in “{distro}” yet")
        } else {
            format!("“{distro}” is reached only from the Windows app")
        }),
    })
}

/// Distro names are checked when a Project is saved: they make safe folder names.
fn folder_name(target: &ExecutionTarget) -> String {
    match target {
        ExecutionTarget::ThisMachine => "this-machine".into(),
        ExecutionTarget::Wsl { distro } => format!("wsl-{distro}"),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use localhost_core::{Key, MemorySecretStore, NewService, Origin, ProjectLibrary, ServiceState, Targets};

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn a_wsl_project_says_where_it_can_run() {
        let root = std::env::temp_dir().join(format!("localhost-targets-{}", localhost_core::Id::new()));
        std::fs::create_dir_all(&root).unwrap();
        let library =
            std::sync::Arc::new(ProjectLibrary::open(&root, std::sync::Arc::new(MemorySecretStore::new())).unwrap());
        let wsl = ExecutionTarget::Wsl { distro: "Ubuntu".into() };
        let services = vec![NewService::new("Web", Key::new("web").unwrap(), "sleep 30")];
        let project = library
            .add_project_on(wsl, "Shop", Key::new("shop").unwrap(), "/home/me/shop", services, Origin::Cli)
            .unwrap();
        let targets = Targets::open(library, &root, Default::default(), launcher_with(&root, None));

        let error = targets.start(project.services[0].id, None, Origin::MainWindow).await.unwrap_err();
        assert_eq!(
            error.message(),
            "localhost can’t reach the WSL distro: “Ubuntu” is reached only from the Windows app."
        );
        assert_eq!(targets.status(project.services[0].id).state, ServiceState::Stopped);
        let _ = std::fs::remove_dir_all(root);
    }
}
