//! The `localhostd` binary, driven over its standard input and output as
//! the Windows app drives it inside a WSL distro.
#![cfg(unix)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use localhost_core::remote::{Hello, Remote};
use localhost_core::{Key, MemorySecretStore, NewService, Origin, ProjectLibrary, ServiceEvent, ServiceState};

const PATIENCE: Duration = Duration::from_secs(20);

fn localhostd(storage_root: &std::path::Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_localhostd"));
    command.arg("--stdio").env("LOCALHOST_STORAGE_ROOT", storage_root);
    command
}

fn group_exists(group: i32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", "--", &format!("-{group}")])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}

#[tokio::test(flavor = "multi_thread")]
async fn runs_a_service_until_its_input_closes() {
    let root = tempfile::tempdir().unwrap();
    let library = ProjectLibrary::open(&root.path().join("app"), Arc::new(MemorySecretStore::new())).unwrap();
    let folder = root.path().join("project");
    std::fs::create_dir_all(&folder).unwrap();
    let pid_file = root.path().join("pid");
    let project =
        library.add_project("Shop", Key::new("shop").unwrap(), folder.to_str().unwrap(), vec![], Origin::Cli).unwrap();
    let command = format!("echo $$ > '{}'; sleep 30", pid_file.display());
    let service = library
        .add_service(project.id, NewService::new("Web", Key::new("web").unwrap(), command), vec![], vec![], Origin::Cli)
        .unwrap();

    let remote = Remote::spawn(localhostd(&root.path().join("localhostd"))).await.unwrap();
    assert_eq!(remote.hello(), &Hello::current());
    remote.set_projects(library.projects(), Default::default()).await.unwrap();
    let (_, mut events) = remote.subscribe();
    remote.start(service.id, None, Origin::MainWindow).await.unwrap();
    tokio::time::timeout(PATIENCE, async {
        while let Some(event) = events.recv().await {
            if matches!(&event, ServiceEvent::ServiceStateChanged(status) if status.state == ServiceState::Running) {
                return;
            }
        }
    })
    .await
    .expect("Running");
    let group: i32 = std::fs::read_to_string(&pid_file).unwrap().trim().parse().unwrap();
    assert!(group_exists(group));

    // Records and logs go to its own storage root.
    assert!(root.path().join("localhostd").join("runs").is_dir());

    drop(events);
    drop(remote);
    let deadline = Instant::now() + PATIENCE;
    while group_exists(group) {
        assert!(Instant::now() < deadline, "the Run outlived the closed input");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[test]
fn prints_its_version_and_refuses_other_arguments() {
    let version = std::process::Command::new(env!("CARGO_BIN_EXE_localhostd")).arg("--version").output().unwrap();
    assert_eq!(String::from_utf8(version.stdout).unwrap(), format!("localhostd {}\n", Hello::current().version));
    let usage = std::process::Command::new(env!("CARGO_BIN_EXE_localhostd")).output().unwrap();
    assert_eq!(usage.status.code(), Some(2));
}
