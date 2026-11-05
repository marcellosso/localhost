//! Reaching another Machine through the real `ssh`: a private `sshd` on a
//! loopback port, run as this user with throwaway keys, whose sessions find
//! this build's `localhost` first on the `PATH`. Nothing of the user's own
//! SSH setup is read or written, nor their zsh files, which could change
//! that `PATH`. Skipped where there is no `sshd`.
#![cfg(target_os = "linux")]

use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;

use localhost_core::machine::SshLink;
use localhost_core::{
    Id, Key, LocalhostError, Machine, MachineProblem, MemorySecretStore, NewService, Origin, ProjectLibrary,
    ServiceState,
};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const PATIENCE: Duration = Duration::from_secs(60);
const SSHD: &str = "/usr/sbin/sshd";

/// The other Machine: a home with a Project, and an `sshd` serving it.
struct OtherMachine {
    folder: TempDir,
    sshd: Child,
}

impl Drop for OtherMachine {
    fn drop(&mut self) {
        let _ = self.sshd.kill();
        let _ = self.sshd.wait();
        // Its shared core exits once idle; don't leave it waiting.
        let _ = Command::new("pkill").arg("-f").arg(self.data().to_str().unwrap()).status();
    }
}

fn free_port() -> u16 {
    TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap().local_addr().unwrap().port()
}

fn keygen(path: &Path) {
    let status = Command::new("ssh-keygen").args(["-q", "-t", "ed25519", "-N", ""]).arg("-f").arg(path).status();
    assert!(status.unwrap().success());
}

impl OtherMachine {
    /// `None` where this computer can't run an `sshd`.
    fn start() -> Option<OtherMachine> {
        if !Path::new(SSHD).exists() || Command::new("ssh").arg("-V").output().is_err() {
            eprintln!("skipped: no sshd or ssh here");
            return None;
        }
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path();
        let bin = path.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(path.join("data/localhost")).unwrap();
        // `localhost` as each test needs it: this build, an older one, or none.
        let script = format!(
            "#!/bin/sh\ncase \"$LH_VARIANT\" in\n  old) echo 'Unknown command `remote`.' >&2; echo 'Run `localhost --help` for usage.' >&2; exit 2;;\n  missing) echo 'sh: 1: localhost: not found' >&2; exit 127;;\nesac\nexec {} \"$@\"\n",
            env!("CARGO_BIN_EXE_localhost")
        );
        std::fs::write(bin.join("localhost"), script).unwrap();
        std::fs::set_permissions(bin.join("localhost"), std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        keygen(&path.join("host_key"));
        keygen(&path.join("client_key"));
        std::fs::copy(path.join("client_key.pub"), path.join("authorized_keys")).unwrap();

        let port = free_port();
        let p = path.display();
        std::fs::write(
            path.join("sshd_config"),
            format!(
                "Port {port}\nListenAddress 127.0.0.1\nHostKey {p}/host_key\nAuthorizedKeysFile {p}/authorized_keys\n\
                 PidFile {p}/sshd.pid\nUsePAM no\nStrictModes no\nPasswordAuthentication no\nKbdInteractiveAuthentication no\n\
                 AcceptEnv LH_VARIANT\nSetEnv PATH={p}/bin:/usr/bin:/bin XDG_DATA_HOME={p}/data ZDOTDIR={p}\n"
            ),
        )
        .unwrap();
        let host_key = std::fs::read_to_string(path.join("host_key.pub")).unwrap();
        let host_key: Vec<&str> = host_key.split_whitespace().take(2).collect();
        std::fs::write(path.join("known_hosts"), format!("[127.0.0.1]:{port} {}\n", host_key.join(" "))).unwrap();
        let user = std::env::var("USER").unwrap_or_else(|_| "root".into());
        let host = |name: &str, extra: &str| {
            format!(
                "Host {name}\n  HostName 127.0.0.1\n  Port {port}\n  User {user}\n  IdentityFile {p}/client_key\n  IdentitiesOnly yes\n  \
                 UserKnownHostsFile {p}/known_hosts\n  StrictHostKeyChecking yes\n{extra}"
            )
        };
        let config = [
            host("devbox", ""),
            host("oldbox", "  SetEnv LH_VARIANT=old\n"),
            host("barebox", "  SetEnv LH_VARIANT=missing\n"),
            host("strangebox", "").replace(&format!("{p}/known_hosts"), "/dev/null"),
        ];
        std::fs::write(path.join("ssh_config"), config.join("")).unwrap();
        std::fs::write(path.join("ssh"), format!("#!/bin/sh\nexec ssh -F {p}/ssh_config \"$@\"\n")).unwrap();
        std::fs::set_permissions(path.join("ssh"), std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();

        let sshd = Command::new(SSHD)
            .arg("-D")
            .arg("-f")
            .arg(path.join("sshd_config"))
            .arg("-E")
            .arg(path.join("sshd.log"))
            .spawn()
            .ok()?;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
            if std::time::Instant::now() > deadline {
                eprintln!(
                    "skipped: sshd didn't start: {}",
                    std::fs::read_to_string(path.join("sshd.log")).unwrap_or_default()
                );
                return None;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Some(OtherMachine { folder, sshd })
    }

    fn path(&self) -> &Path {
        self.folder.path()
    }

    fn data(&self) -> PathBuf {
        self.path().join("data")
    }

    fn ssh(&self) -> PathBuf {
        self.path().join("ssh")
    }

    /// Its Project Configuration, as an agent there would have saved it.
    fn library(&self) -> ProjectLibrary {
        ProjectLibrary::open(&self.data().join("localhost"), std::sync::Arc::new(MemorySecretStore::new())).unwrap()
    }

    fn machine(&self, destination: &str) -> Machine {
        Machine { id: Id::new(), name: destination.into(), destination: destination.into() }
    }

    async fn connect(
        &self,
        destination: &str,
    ) -> Result<(std::sync::Arc<SshLink>, localhost_core::machine::MachineConnection), LocalhostError> {
        let (ssh, control) = (self.ssh(), self.path().join("control"));
        let link = SshLink::connect(self.machine(destination), &ssh, &control);
        tokio::time::timeout(PATIENCE, link).await.expect("ssh hung")
    }
}

fn problem(result: Result<impl Sized, LocalhostError>) -> MachineProblem {
    match result {
        Err(LocalhostError::MachineUnavailable { problem, .. }) => problem,
        Err(other) => panic!("not a Machine problem: {other:?}"),
        Ok(_) => panic!("it connected"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn controls_the_other_machines_services_over_ssh() {
    let Some(other) = OtherMachine::start() else { return };
    let library = other.library();
    let folder = other.path().join("app");
    std::fs::create_dir_all(&folder).unwrap();
    let project =
        library.add_project("App", Key::new("app").unwrap(), folder.to_str().unwrap(), vec![], Origin::Agent).unwrap();
    let web = library
        .add_service(
            project.id,
            NewService::new("Web", Key::new("web").unwrap(), "exec sleep 60"),
            vec![],
            vec![],
            Origin::Agent,
        )
        .unwrap();

    let (link, connection) = other.connect("devbox").await.unwrap();
    assert_eq!(connection.hello().version, localhost_core::VERSION);
    let (projects, _) = connection.projects().await.unwrap();
    assert_eq!(projects.iter().map(|project| project.key.as_str()).collect::<Vec<_>>(), ["app"]);

    let status = connection.start(web.id, None).await.unwrap();
    assert_eq!(status.run.unwrap().origin, Origin::Machine);
    let (statuses, mut events) = connection.subscribe().await.unwrap();
    if statuses.get(&web.id).map(|status| status.state) != Some(ServiceState::Running) {
        tokio::time::timeout(PATIENCE, async {
            while let Some(event) = events.recv().await {
                if let localhost_core::ServiceEvent::ServiceStateChanged(status) = event
                    && status.state == ServiceState::Running
                {
                    return;
                }
            }
        })
        .await
        .expect("it never ran");
    }
    assert_eq!(connection.stop(web.id).await.unwrap().state, ServiceState::Stopped);

    // Ending the link ends the connection, and the reverse.
    link.end();
    tokio::time::timeout(PATIENCE, connection.closed()).await.expect("still connected");
}

#[tokio::test(flavor = "multi_thread")]
async fn forwards_a_port_of_the_other_machine_to_this_one() {
    let Some(other) = OtherMachine::start() else { return };
    let (link, _connection) = other.connect("devbox").await.unwrap();

    // A dev server there; here both ends are this computer, so the Forward
    // takes another local port.
    let server = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let port = server.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = server.accept().await {
            tokio::spawn(async move {
                let mut buffer = [0u8; 4];
                if stream.read_exact(&mut buffer).await.is_ok() {
                    let _ = stream.write_all(b"pong").await;
                }
            });
        }
    });

    // A port in use here is a conflict, never a Forward beside it.
    let taken = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let taken_port = taken.local_addr().unwrap().port();
    assert!(matches!(
        link.forward_to(port, taken_port).await,
        Err(LocalhostError::PortConflict { port, .. }) if port == taken_port
    ));

    let local_port = free_port();
    let forward = link.forward_to(port, local_port).await.unwrap();
    assert_eq!(link.forwards(), [forward]);
    let mut stream = tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, local_port)).await.unwrap();
    stream.write_all(b"ping").await.unwrap();
    let mut reply = [0u8; 4];
    tokio::time::timeout(PATIENCE, stream.read_exact(&mut reply)).await.unwrap().unwrap();
    assert_eq!(&reply, b"pong");

    link.cancel_forward(forward).await;
    assert!(link.forwards().is_empty());
    assert!(std::net::TcpStream::connect((Ipv4Addr::LOCALHOST, local_port)).is_err());
}

/// A Unix socket's path is limited to about 104 bytes, and macOS's
/// temporary and home folders can be long.
#[tokio::test(flavor = "multi_thread")]
async fn keeps_its_control_socket_in_a_folder_of_any_length() {
    let Some(other) = OtherMachine::start() else { return };
    let control =
        other.path().join("a-folder-whose-path-is-longer-than-a-unix-socket-path-may-be").join("x".repeat(60));
    assert!(control.as_os_str().len() > 120);
    let ssh = other.ssh();
    let link = SshLink::connect(other.machine("devbox"), &ssh, &control);
    let (link, connection) = tokio::time::timeout(PATIENCE, link).await.expect("ssh hung").unwrap();
    assert!(connection.is_connected());

    let server = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = server.local_addr().unwrap().port();
    let local_port = free_port();
    let forward = link.forward_to(port, local_port).await.unwrap();
    assert!(std::net::TcpStream::connect((Ipv4Addr::LOCALHOST, local_port)).is_ok());
    link.cancel_forward(forward).await;
    assert!(std::net::TcpStream::connect((Ipv4Addr::LOCALHOST, local_port)).is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn says_why_a_machine_cant_be_reached() {
    let Some(other) = OtherMachine::start() else { return };
    assert_eq!(problem(other.connect("strangebox").await), MachineProblem::HostKeyNotTrusted);
    assert_eq!(problem(other.connect("oldbox").await), MachineProblem::TooOld(None));
    assert_eq!(problem(other.connect("barebox").await), MachineProblem::NotInstalled);
}

#[tokio::test(flavor = "multi_thread")]
async fn connects_again_after_the_connection_is_lost() {
    use localhost_core::machine::{MachineState, Machines, MachinesEvent};
    let Some(other) = OtherMachine::start() else { return };
    let app_root = other.path().join("app-storage");
    let machines = Machines::open(&app_root, &other.ssh()).unwrap();
    let devbox = machines.add(None, "devbox").unwrap();
    let (_, _, mut events) = machines.subscribe();
    let connected = |event: &MachinesEvent| {
        matches!(event, MachinesEvent::StateChanged { state: MachineState::Connected { .. }, .. })
    };
    machines.connect(devbox.id).unwrap();
    until_event(&mut events, "connected", connected).await;
    let (projects, _) = machines.connection(devbox.id).unwrap().projects().await.unwrap();
    assert!(projects.is_empty());

    // The network drops, or the laptop slept past the heartbeat.
    machines.connection(devbox.id).unwrap().close();
    let lost = |event: &MachinesEvent| {
        matches!(
            event,
            MachinesEvent::StateChanged {
                state: MachineState::Unavailable { problem: MachineProblem::Lost, is_retrying: true },
                ..
            }
        )
    };
    until_event(&mut events, "lost", lost).await;
    until_event(&mut events, "reconnected", connected).await;
    assert!(machines.connection(devbox.id).is_ok());

    machines.disconnect(devbox.id);
    assert_eq!(machines.state(devbox.id), MachineState::NotConnected);
    assert!(machines.connection(devbox.id).is_err());
}

async fn until_event(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<localhost_core::machine::MachinesEvent>,
    what: &str,
    condition: impl Fn(&localhost_core::machine::MachinesEvent) -> bool,
) {
    let wait = async {
        while let Some(event) = events.recv().await {
            if condition(&event) {
                return;
            }
        }
    };
    tokio::time::timeout(PATIENCE, wait).await.unwrap_or_else(|_| panic!("never {what}"));
}
