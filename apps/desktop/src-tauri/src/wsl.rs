//! WSL distros, as the Windows app reaches them (ADR-0008): their names,
//! the paths Windows sees their folders at, and the `localhostd` each one
//! runs.
//!
//! The app copies its `localhostd` (a Linux build) into each distro it
//! uses, at `~/.local/share/localhost/windows/`, named after the app's
//! version and a hash of the binary. A distro without that exact copy gets
//! it, and older copies are removed: the distro always runs the app's own
//! `localhostd`. Its Runs' logs and records are kept in the same folder.

use std::path::PathBuf;

use localhost_core::ExecutionTarget;

/// Where `localhostd` and its data live, under the distro user's home.
const FOLDER: &str = ".local/share/localhost/windows";

/// What `sh` runs in a distro, with `$1` the name of the app's copy of
/// `localhostd` (`localhostd_name`).
#[cfg_attr(not(windows), allow(dead_code))]
mod script {
    use super::FOLDER;

    /// Exits with 0 if the distro has the copy, 1 if not.
    pub fn has() -> String {
        format!(r#"test -x "$HOME/{FOLDER}/$1""#)
    }

    /// Writes the copy from its input, then removes any other.
    pub fn install() -> String {
        format!(
            r#"set -e; d="$HOME/{FOLDER}"; mkdir -p "$d"; cat > "$d/.$1.part"; chmod 755 "$d/.$1.part"; mv -f "$d/.$1.part" "$d/$1"; for f in "$d"/localhostd-*; do [ "$f" = "$d/$1" ] || rm -f "$f"; done"#
        )
    }

    /// Runs the copy over its input and output, its data beside it.
    pub fn run() -> String {
        format!(r#"d="$HOME/{FOLDER}"; LOCALHOST_STORAGE_ROOT="$d" exec "$d/$1" --stdio"#)
    }
}

/// The distros `wsl.exe --list --quiet` printed, in its order (the default
/// first), without Docker Desktop's own.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn distro_names(output: &[u8]) -> Vec<String> {
    decode(output)
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty() && !name.starts_with("docker-desktop"))
        .map(String::from)
        .collect()
}

/// `wsl.exe` writes UTF-16 unless it honours `WSL_UTF8`, which older
/// versions don't. UTF-8 text never has a zero byte.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn decode(output: &[u8]) -> String {
    let text = if output.contains(&0) {
        let units: Vec<u16> = output.chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(output).into_owned()
    };
    text.trim_start_matches('\u{feff}').to_string()
}

/// The distro and Linux path of a folder Windows sees in a distro:
/// `\\wsl.localhost\Ubuntu\home\me` (or `\\wsl$\…`) is `/home/me` in Ubuntu.
pub fn linux_path(windows: &str) -> Option<(String, String)> {
    let path = windows.replace('/', "\\");
    let path = path.strip_prefix(r"\\?\UNC\").map_or(path.clone(), |rest| format!(r"\\{rest}"));
    let (host, rest) = path.strip_prefix(r"\\")?.split_once('\\').unwrap_or((&path[2..], ""));
    if !host.eq_ignore_ascii_case("wsl.localhost") && !host.eq_ignore_ascii_case("wsl$") {
        return None;
    }
    let (distro, rest) = rest.split_once('\\').unwrap_or((rest, ""));
    if distro.is_empty() {
        return None;
    }
    Some((distro.to_string(), posix(rest.split('\\'))))
}

/// Where a distro sees a folder on a Windows drive: `C:\Users\me` is
/// `/mnt/c/Users/me`, WSL's default mount.
pub fn mounted_path(windows: &str) -> Option<String> {
    let mut characters = windows.chars();
    let drive = characters.next().filter(char::is_ascii_alphabetic)?;
    let rest = characters.as_str().strip_prefix(':')?;
    if !rest.is_empty() && !rest.starts_with(['\\', '/']) {
        return None;
    }
    let path = posix(rest.split(['\\', '/']));
    Some(format!("/mnt/{}{}", drive.to_ascii_lowercase(), if path == "/" { "" } else { &path }))
}

/// Where Windows sees a distro's folder: `\\wsl.localhost\Ubuntu\home\me`,
/// or `C:\Users\me` for `/mnt/c/Users/me`.
pub fn windows_path(distro: &str, linux: &str) -> String {
    let parts: Vec<&str> = linux.split('/').filter(|part| !part.is_empty()).collect();
    match parts.as_slice() {
        ["mnt", drive, rest @ ..] if drive.len() == 1 && drive.chars().all(|c| c.is_ascii_alphabetic()) => {
            format!(r"{}:\{}", drive.to_ascii_uppercase(), rest.join("\\"))
        }
        _ => format!(r"\\wsl.localhost\{distro}\{}", parts.join("\\")),
    }
}

/// Where this app sees a folder of `target`: a distro's through
/// `\\wsl.localhost`, on Windows only. `None` when it can't.
pub fn local_path(target: &ExecutionTarget, path: &str) -> Option<PathBuf> {
    match target {
        ExecutionTarget::ThisMachine => Some(PathBuf::from(path)),
        ExecutionTarget::Wsl { distro } if cfg!(windows) => Some(PathBuf::from(windows_path(distro, path))),
        ExecutionTarget::Wsl { .. } => None,
    }
}

/// `relative` (e.g. `web` or `../shared`) inside `base`, a distro's absolute
/// path, wherever this app runs.
pub fn joined(base: &str, relative: &str) -> String {
    posix(base.split('/').chain(relative.split('/')))
}

/// An absolute POSIX path of `parts`, without `.` and with `..` resolved.
fn posix<'a>(parts: impl Iterator<Item = &'a str>) -> String {
    let mut kept: Vec<&str> = Vec::new();
    for part in parts {
        match part {
            "" | "." => {}
            ".." => {
                kept.pop();
            }
            part => kept.push(part),
        }
    }
    format!("/{}", kept.join("/"))
}

/// The name of the app's `localhostd` in a distro: its version, and a hash
/// of the binary, so a development build that changed is copied again.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn localhostd_name(binary: &[u8]) -> String {
    // FNV-1a: no cryptography needed, only a change in the bytes.
    let hash = binary
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3));
    format!("localhostd-{}-{hash:016x}", env!("CARGO_PKG_VERSION"))
}

/// The distros a Project can run in, for its editor: none without WSL, or
/// off Windows.
#[tauri::command]
#[specta::specta]
pub async fn wsl_distros() -> Vec<String> {
    distros().await
}

async fn distros() -> Vec<String> {
    #[cfg(windows)]
    if let Ok(output) = windows::wsl().args(["--list", "--quiet"]).output().await
        && output.status.success()
    {
        return distro_names(&output.stdout);
    }
    Vec::new()
}

#[cfg(windows)]
pub use windows::connect;

#[cfg(windows)]
mod windows {
    use std::path::PathBuf;
    use std::process::{Output, Stdio};

    use localhost_core::LocalhostError;
    use localhost_core::remote::Remote;
    use tokio::io::AsyncWriteExt;
    use tokio::process::Command;

    use super::{decode, distros, localhostd_name, script};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// The development override for the `localhostd` to copy: a Linux build,
    /// e.g. `target/x86_64-unknown-linux-musl/debug/localhostd`.
    const WSL_LOCALHOSTD: &str = "LOCALHOST_WSL_LOCALHOSTD";

    /// `wsl.exe`, without a console window, writing UTF-8 where it can.
    pub(super) fn wsl() -> Command {
        let mut command = Command::new("wsl.exe");
        command.env("WSL_UTF8", "1").creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null());
        command
    }

    /// `script` run by `sh` in the distro, with `$1` set to `argument`.
    fn in_distro(distro: &str, script: &str, argument: &str) -> Command {
        let mut command = wsl();
        command.args(["--distribution", distro, "--cd", "~", "--exec", "/bin/sh", "-c", script, "sh", argument]);
        command
    }

    /// Starts the app's `localhostd --stdio` in the distro, copying it in
    /// first if the distro doesn't have it.
    pub async fn connect(distro: String) -> Result<Remote, LocalhostError> {
        if !distros().await.contains(&distro) {
            return Err(unavailable(format!("no distro named “{distro}” is installed")));
        }
        let source = bundled_localhostd().ok_or_else(|| unavailable("this app has no localhostd to run in it"))?;
        let binary = tokio::fs::read(&source)
            .await
            .map_err(|error| unavailable(format!("localhostd can’t be read from {}: {error}", source.display())))?;
        let name = localhostd_name(&binary);
        if !has(&distro, &name).await? {
            install(&distro, &name, &binary).await?;
        }
        Remote::spawn(in_distro(&distro, &script::run(), &name)).await
    }

    /// `LOCALHOST_WSL_LOCALHOSTD`, else the one installed beside the app.
    fn bundled_localhostd() -> Option<PathBuf> {
        if let Some(path) = std::env::var_os(WSL_LOCALHOSTD) {
            return Some(path.into());
        }
        Some(std::env::current_exe().ok()?.parent()?.join("localhostd"))
    }

    async fn has(distro: &str, name: &str) -> Result<bool, LocalhostError> {
        let output = in_distro(distro, &script::has(), name).output().await.map_err(wsl_failed)?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(unavailable(said(&output))),
        }
    }

    /// Copies the binary in through `wsl.exe`'s input, then removes other copies.
    async fn install(distro: &str, name: &str, binary: &[u8]) -> Result<(), LocalhostError> {
        let mut command = in_distro(distro, &script::install(), name);
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command.spawn().map_err(wsl_failed)?;
        let mut input = child.stdin.take().expect("piped");
        let written = input.write_all(binary).await;
        drop(input);
        let output = child.wait_with_output().await.map_err(wsl_failed)?;
        if written.is_err() || !output.status.success() {
            return Err(unavailable(format!("localhostd couldn’t be copied into it: {}", said(&output))));
        }
        Ok(())
    }

    /// What `wsl.exe` or the distro said, as the end of a sentence.
    fn said(output: &Output) -> String {
        let text = [decode(&output.stdout), decode(&output.stderr)].join(" ");
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let text = text.trim_end_matches('.');
        if text.is_empty() { format!("wsl.exe exited with {}", output.status) } else { text.to_string() }
    }

    fn wsl_failed(error: std::io::Error) -> LocalhostError {
        unavailable(format!("wsl.exe could not be started ({error}); is WSL installed?"))
    }

    fn unavailable(reason: impl Into<String>) -> LocalhostError {
        LocalhostError::TargetUnavailable(reason.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_distro_list_in_either_encoding() {
        let utf16: Vec<u8> =
            "\u{feff}Ubuntu\r\ndocker-desktop\r\nDebian\r\n\r\n".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(distro_names(&utf16), ["Ubuntu", "Debian"]);
        assert_eq!(distro_names(b"Ubuntu-24.04\nkali-linux\n"), ["Ubuntu-24.04", "kali-linux"]);
        assert!(distro_names(b"").is_empty());
    }

    #[test]
    fn maps_a_distro_folder_to_its_linux_path_and_back() {
        let ubuntu = |path: &str| Some(("Ubuntu".to_string(), path.to_string()));
        assert_eq!(linux_path(r"\\wsl.localhost\Ubuntu\home\me\shop"), ubuntu("/home/me/shop"));
        assert_eq!(linux_path(r"\\wsl$\Ubuntu\home\me\shop\"), ubuntu("/home/me/shop"));
        assert_eq!(linux_path(r"\\?\UNC\wsl.localhost\Ubuntu\home"), ubuntu("/home"));
        assert_eq!(linux_path(r"\\WSL.LOCALHOST\Ubuntu"), ubuntu("/"));
        assert_eq!(linux_path(r"\\server\share\shop"), None);
        assert_eq!(linux_path(r"C:\Users\me"), None);
        assert_eq!(linux_path("/home/me"), None);

        assert_eq!(windows_path("Ubuntu", "/home/me/shop"), r"\\wsl.localhost\Ubuntu\home\me\shop");
        assert_eq!(windows_path("Ubuntu", "/"), r"\\wsl.localhost\Ubuntu\");
        assert_eq!(windows_path("Ubuntu", "/mnt/c/Users/me"), r"C:\Users\me");
        assert_eq!(windows_path("Ubuntu", "/mnt/data/shop"), r"\\wsl.localhost\Ubuntu\mnt\data\shop");
    }

    #[test]
    fn sees_a_distro_folder_from_windows_only() {
        let ubuntu = ExecutionTarget::Wsl { distro: "Ubuntu".into() };
        assert_eq!(local_path(&ExecutionTarget::ThisMachine, "/home/me"), Some(PathBuf::from("/home/me")));
        let expected = cfg!(windows).then(|| PathBuf::from(r"\\wsl.localhost\Ubuntu\home\me"));
        assert_eq!(local_path(&ubuntu, "/home/me"), expected);
    }

    #[test]
    fn joins_a_working_directory_inside_a_distro() {
        assert_eq!(joined("/home/me/shop", "."), "/home/me/shop");
        assert_eq!(joined("/home/me/shop", "apps/web"), "/home/me/shop/apps/web");
        assert_eq!(joined("/home/me/shop", "../shared"), "/home/me/shared");
    }

    #[test]
    fn maps_a_windows_drive_folder_to_where_a_distro_mounts_it() {
        assert_eq!(mounted_path(r"C:\Users\me\shop"), Some("/mnt/c/Users/me/shop".into()));
        assert_eq!(mounted_path(r"d:\"), Some("/mnt/d".into()));
        assert_eq!(mounted_path("D:"), Some("/mnt/d".into()));
        assert_eq!(mounted_path(r"\\wsl.localhost\Ubuntu\home"), None);
        assert_eq!(mounted_path("/home/me"), None);
        assert_eq!(mounted_path("C:relative"), None);
    }

    /// The scripts as a distro runs them, with `sh` here and a home of their own.
    #[cfg(unix)]
    #[test]
    fn installs_the_apps_localhostd_over_any_other_and_runs_it() {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let home = std::env::temp_dir().join(format!("localhost-wsl-{}", localhost_core::Id::new()));
        std::fs::create_dir_all(&home).unwrap();
        let sh = |script: String, name: &str, input: &[u8]| {
            let mut child = Command::new("/bin/sh")
                .args(["-c", &script, "sh", name])
                .env("HOME", &home)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(input).unwrap();
            child.wait_with_output().unwrap()
        };
        let fake = b"#!/bin/sh\necho \"$LOCALHOST_STORAGE_ROOT $*\"\n";
        let name = localhostd_name(fake);

        assert_eq!(sh(script::has(), &name, b"").status.code(), Some(1));
        assert!(sh(script::install(), "localhostd-0.0.1-0123456789abcdef", b"an older one").status.success());
        assert!(sh(script::install(), &name, fake).status.success());
        assert_eq!(sh(script::has(), &name, b"").status.code(), Some(0));
        let folder = home.join(FOLDER);
        let copies: Vec<_> = std::fs::read_dir(&folder).unwrap().map(|entry| entry.unwrap().file_name()).collect();
        assert_eq!(copies, [name.as_str()], "only this app's copy is kept");

        let output = sh(script::run(), &name, b"");
        assert_eq!(String::from_utf8_lossy(&output.stdout), format!("{} --stdio\n", folder.display()));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn names_each_build_of_localhostd_apart() {
        let name = localhostd_name(b"one build");
        assert!(name.starts_with(&format!("localhostd-{}-", env!("CARGO_PKG_VERSION"))), "{name}");
        assert_eq!(name, localhostd_name(b"one build"));
        assert_ne!(name, localhostd_name(b"another build"));
    }
}
