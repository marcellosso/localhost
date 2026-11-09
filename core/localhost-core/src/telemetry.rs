//! Anonymous usage telemetry for the apps, to measure retention: two events,
//! `app_first_open` and `app_opened`, sent to PostHog's capture API with an
//! anonymous id, the version, OS, architecture and install channel, and
//! nothing else.
//!
//! Only the apps call this, and only they build the core with the
//! `telemetry` feature that can send: the `localhost` command and
//! `localhostd` never do. It sends nothing unless it has a PostHog key, in
//! a release build, outside CI, and when neither the user
//! (Settings) nor the environment (`DO_NOT_TRACK`, `LOCALHOST_TELEMETRY=0`)
//! turned it off.
//!
//! What it remembers is in `telemetry.json` in the storage root, so the user's
//! choice is the same whichever app reads it.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::LocalhostError;

/// The app PostHog project's public token. It can only send events, so it
/// is safe to ship; debug builds, tests and CI never use it.
const DEFAULT_KEY: &str = "phc_rwtSF5BwJNJemjT32Z4EK9PkVEwSD8LroksUP7cZpTzY";
/// `LOCALHOST_POSTHOG_KEY` at build time replaces it; set but empty, the build sends nothing.
const BUILD_KEY: Option<&str> = match option_env!("LOCALHOST_POSTHOG_KEY") {
    Some(key) => Some(key),
    None => Some(DEFAULT_KEY),
};
/// Where events go; a first-party proxy can replace PostHog's host without a code change.
const BUILD_HOST: Option<&str> = option_env!("LOCALHOST_POSTHOG_HOST");
const DEFAULT_HOST: &str = "https://us.i.posthog.com";

const FILE_NAME: &str = "telemetry.json";

/// The privacy page the notice and Settings link to.
pub const PRIVACY_URL: &str = "https://www.getlocalhost.app/privacy";

/// The events the apps send, and the only ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetryEvent {
    /// The first open of this install that was sent.
    FirstOpen,
    /// Every open after it.
    Opened,
}

impl TelemetryEvent {
    pub fn name(self) -> &'static str {
        match self {
            TelemetryEvent::FirstOpen => "app_first_open",
            TelemetryEvent::Opened => "app_opened",
        }
    }
}

/// How this copy of the app was installed, guessed from where it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallChannel {
    Brew,
    Dmg,
    Exe,
    Deb,
    Rpm,
    AppImage,
    Source,
    Unknown,
}

impl InstallChannel {
    pub fn name(self) -> &'static str {
        match self {
            InstallChannel::Brew => "brew",
            InstallChannel::Dmg => "dmg",
            InstallChannel::Exe => "exe",
            InstallChannel::Deb => "deb",
            InstallChannel::Rpm => "rpm",
            InstallChannel::AppImage => "appimage",
            InstallChannel::Source => "source",
            InstallChannel::Unknown => "unknown",
        }
    }

    /// This running app's channel. Only the name leaves the machine, never a path.
    pub fn detect() -> InstallChannel {
        let Ok(executable) = std::env::current_exe() else { return InstallChannel::Unknown };
        let executable = fs::canonicalize(&executable).unwrap_or(executable);
        Self::detect_from(&executable, &|name| std::env::var_os(name).is_some(), &|path| path.exists())
    }

    fn detect_from(executable: &Path, is_set: &dyn Fn(&str) -> bool, exists: &dyn Fn(&Path) -> bool) -> InstallChannel {
        if cfg!(target_os = "macos") {
            // The cask moves the app into /Applications and keeps its record in the Caskroom.
            let casked = ["/opt/homebrew/Caskroom/localhost", "/usr/local/Caskroom/localhost"]
                .iter()
                .any(|path| exists(Path::new(path)));
            let in_bundle = executable.components().any(|part| part.as_os_str().to_string_lossy().ends_with(".app"));
            match (in_bundle, casked) {
                (true, true) => InstallChannel::Brew,
                (true, false) => InstallChannel::Dmg,
                (false, _) => InstallChannel::Source,
            }
        } else if cfg!(windows) {
            // The NSIS installer leaves its uninstaller beside the app.
            match executable.parent() {
                Some(folder) if exists(&folder.join("uninstall.exe")) => InstallChannel::Exe,
                Some(_) => InstallChannel::Source,
                None => InstallChannel::Unknown,
            }
        } else {
            if is_set("APPIMAGE") {
                return InstallChannel::AppImage;
            }
            if !executable.starts_with("/usr") {
                return InstallChannel::Source;
            }
            if exists(Path::new("/var/lib/dpkg/info/localhost.list")) {
                InstallChannel::Deb
            } else if exists(Path::new("/var/lib/rpm")) || exists(Path::new("/usr/lib/sysimage/rpm")) {
                InstallChannel::Rpm
            } else {
                InstallChannel::Unknown
            }
        }
    }
}

/// What the build and the environment allow, before the user's choice.
#[derive(Clone, Debug)]
struct Allowance {
    /// The PostHog key and host, when this build may send at all.
    destination: Option<(String, String)>,
}

impl Allowance {
    /// This build in this process's environment.
    fn current() -> Allowance {
        let key = if cfg!(feature = "telemetry") { BUILD_KEY } else { None };
        Allowance::new(key, BUILD_HOST, !cfg!(debug_assertions), &|name| std::env::var(name).ok())
    }

    fn new(
        key: Option<&str>,
        host: Option<&str>,
        is_release: bool,
        variable: &dyn Fn(&str) -> Option<String>,
    ) -> Allowance {
        let key = key.map(str::trim).filter(|key| !key.is_empty());
        let host = host.map(str::trim).filter(|host| !host.is_empty()).unwrap_or(DEFAULT_HOST);
        let allowed = is_release && !environment_opts_out(variable);
        Allowance { destination: key.filter(|_| allowed).map(|key| (key.to_string(), host.to_string())) }
    }
}

/// `DO_NOT_TRACK` set to anything but a "no", `LOCALHOST_TELEMETRY` set to
/// a "no", or `CI` set (tests and builds in CI never send).
fn environment_opts_out(variable: &dyn Fn(&str) -> Option<String>) -> bool {
    let is_no = |value: &str| ["", "0", "false", "no", "off"].contains(&value.trim().to_ascii_lowercase().as_str());
    let is_yes = |name: &str| variable(name).is_some_and(|value| !is_no(&value));
    is_yes("DO_NOT_TRACK") || is_yes("CI") || variable("LOCALHOST_TELEMETRY").is_some_and(|value| is_no(&value))
}

/// What is remembered between launches.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
    /// Created the first time an event is sent, then kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<Uuid>,
    #[serde(default = "yes")]
    enabled: bool,
    #[serde(default)]
    notice_shown: bool,
    #[serde(default)]
    first_open_sent: bool,
}

fn yes() -> bool {
    true
}

impl Default for Stored {
    fn default() -> Stored {
        Stored { id: None, enabled: true, notice_shown: false, first_open_sent: false }
    }
}

/// Sends one request: the URL and the JSON body. True when it was accepted.
pub(crate) type Sender = dyn Fn(&str, &[u8]) -> bool + Send + Sync;

/// The apps' telemetry: the user's choice, the first-open notice, and the
/// open events. Every call returns at once; sending happens on a thread of
/// its own and its failures are dropped.
#[cfg_attr(feature = "ffi", derive(uniffi::Object))]
pub struct Telemetry {
    path: PathBuf,
    stored: Mutex<Stored>,
    /// The file exists but couldn't be read: it is left alone and nothing is sent,
    /// since it may hold the user's "off".
    is_unreadable: bool,
    allowance: Allowance,
    channel: fn() -> InstallChannel,
    sender: Arc<Sender>,
}

impl Telemetry {
    /// The telemetry state kept in `storage_root`.
    pub fn open(storage_root: &Path) -> Telemetry {
        Telemetry::with(storage_root, Allowance::current(), InstallChannel::detect, Arc::new(post))
    }

    fn with(
        storage_root: &Path,
        allowance: Allowance,
        channel: fn() -> InstallChannel,
        sender: Arc<Sender>,
    ) -> Telemetry {
        let path = storage_root.join(FILE_NAME);
        let (stored, is_unreadable) = match fs::read(&path) {
            Ok(data) => match serde_json::from_slice(&data) {
                Ok(stored) => (stored, false),
                Err(_) => (Stored { enabled: false, ..Stored::default() }, true),
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => (Stored::default(), false),
            Err(_) => (Stored { enabled: false, ..Stored::default() }, true),
        };
        Telemetry { path, stored: Mutex::new(stored), is_unreadable, allowance, channel, sender }
    }

    /// Whether the user lets localhost send anonymous usage data (Settings). On by default.
    pub fn is_enabled(&self) -> bool {
        self.stored.lock().unwrap().enabled
    }

    /// Turning it either way also counts as having seen the notice.
    pub fn set_enabled(&self, enabled: bool) -> Result<(), LocalhostError> {
        self.update(|stored| {
            stored.enabled = enabled;
            stored.notice_shown = true;
        })
    }

    /// Whether the first-open notice should show: once per install, and only
    /// in a build and environment that would send.
    pub fn is_notice_pending(&self) -> bool {
        let stored = self.stored.lock().unwrap();
        !stored.notice_shown && !self.is_unreadable && self.allowance.destination.is_some()
    }

    /// The notice was shown and dismissed; for this session only if the file can't be written.
    pub fn acknowledge_notice(&self) {
        let _ = self.update(|stored| stored.notice_shown = true);
    }

    /// Sends `app_first_open` the first time, `app_opened` after it, in the
    /// background, when allowed. Returns at once.
    pub fn record_app_open(self: &Arc<Self>) {
        if self.allowance.destination.is_none() || self.is_unreadable || !self.is_enabled() {
            return;
        }
        let telemetry = Arc::clone(self);
        let _ = std::thread::Builder::new().name("telemetry".into()).spawn(move || telemetry.send_app_open());
    }

    /// The sending part of `record_app_open`, on the caller's thread.
    fn send_app_open(&self) -> Option<TelemetryEvent> {
        let (key, host) = self.allowance.destination.as_ref().filter(|_| !self.is_unreadable)?;
        let (id, event) = {
            let mut stored = self.stored.lock().unwrap();
            if !stored.enabled {
                return None;
            }
            let id = *stored.id.get_or_insert_with(Uuid::new_v4);
            (id, if stored.first_open_sent { TelemetryEvent::Opened } else { TelemetryEvent::FirstOpen })
        };
        // The id is kept before sending, so a failed first send still reuses it.
        let _ = self.update(|_| {});
        let body = payload(key, event, id, (self.channel)());
        let url = format!("{}/i/v0/e/", host.trim_end_matches('/'));
        let body = serde_json::to_vec(&body).expect("a payload always encodes");
        if !(self.sender)(&url, &body) {
            return None;
        }
        if event == TelemetryEvent::FirstOpen {
            let _ = self.update(|stored| stored.first_open_sent = true);
        }
        Some(event)
    }

    /// Changes the stored state and writes it atomically. An unreadable file
    /// is never overwritten: the change then lasts for this session.
    fn update(&self, change: impl FnOnce(&mut Stored)) -> Result<(), LocalhostError> {
        let mut stored = self.stored.lock().unwrap();
        change(&mut stored);
        if self.is_unreadable {
            return Ok(());
        }
        let failure = |error: io::Error| LocalhostError::PersistenceFailed {
            path: self.path.display().to_string(),
            reason: error.to_string(),
        };
        if let Some(folder) = self.path.parent() {
            fs::create_dir_all(folder).map_err(failure)?;
        }
        let mut data = serde_json::to_vec_pretty(&*stored).expect("telemetry state always encodes");
        data.push(b'\n');
        let temporary = self.path.with_file_name(format!(".{FILE_NAME}.tmp-{}", Uuid::new_v4()));
        let result = (|| {
            let mut file = fs::File::create(&temporary)?;
            file.write_all(&data)?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(&temporary);
            return Err(failure(error));
        }
        Ok(())
    }
}

/// A PostHog capture request. Every property is listed here; nothing about
/// the user's Projects, files, machine name or account is ever added.
fn payload(key: &str, event: TelemetryEvent, id: Uuid, channel: InstallChannel) -> serde_json::Value {
    json!({
        "api_key": key,
        "event": event.name(),
        "distinct_id": id.to_string(),
        "properties": {
            "app_version": crate::VERSION,
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "install_channel": channel.name(),
            "product": "localhost-app",
            // Anonymous events: no person profile, no location from the IP address.
            "$process_person_profile": false,
            "$geoip_disable": true,
        },
    })
}

#[cfg(feature = "telemetry")]
fn post(url: &str, body: &[u8]) -> bool {
    // Long enough for a slow network, short enough that a stuck send ends soon after launch.
    let timeout = std::time::Duration::from_secs(5);
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(timeout)).build().into();
    agent.post(url).header("Content-Type", "application/json").send(body).is_ok()
}

/// Without the `telemetry` feature there is no HTTP client to send with.
#[cfg(not(feature = "telemetry"))]
fn post(_url: &str, _body: &[u8]) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    type Requests = Arc<Mutex<Vec<(String, serde_json::Value)>>>;

    fn environment(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let values: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |name| values.get(name).cloned()
    }

    fn allowed() -> Allowance {
        Allowance::new(Some("phc_test"), None, true, &environment(&[]))
    }

    /// Telemetry whose requests are kept instead of sent.
    fn telemetry(root: &Path, allowance: Allowance, accept: bool) -> (Arc<Telemetry>, Requests) {
        let requests: Requests = Arc::default();
        let kept = Arc::clone(&requests);
        let sender = Arc::new(move |url: &str, body: &[u8]| {
            kept.lock().unwrap().push((url.to_string(), serde_json::from_slice(body).unwrap()));
            accept
        });
        (Arc::new(Telemetry::with(root, allowance, || InstallChannel::Dmg, sender)), requests)
    }

    #[test]
    fn a_missing_or_empty_key_never_sends() {
        for key in [None, Some(""), Some("  ")] {
            let allowance = Allowance::new(key, None, true, &environment(&[]));
            assert!(allowance.destination.is_none(), "{key:?}");
            let root = tempfile::tempdir().unwrap();
            let (telemetry, requests) = telemetry(root.path(), allowance, true);
            assert_eq!(telemetry.send_app_open(), None);
            assert!(!telemetry.is_notice_pending());
            assert!(requests.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn debug_builds_never_send() {
        assert!(Allowance::new(Some("phc_test"), None, false, &environment(&[])).destination.is_none());
        // This test binary is a debug build: the real allowance is off too.
        if cfg!(debug_assertions) {
            assert!(Allowance::current().destination.is_none());
        }
    }

    #[test]
    fn the_environment_opts_out() {
        let opted_out = |pairs: &[(&str, &str)]| {
            Allowance::new(Some("phc_test"), None, true, &environment(pairs)).destination.is_none()
        };
        for value in ["1", "true", "yes", "TRUE"] {
            assert!(opted_out(&[("DO_NOT_TRACK", value)]), "DO_NOT_TRACK={value}");
            assert!(opted_out(&[("CI", value)]), "CI={value}");
        }
        for value in ["0", "false", "off", "no", "OFF"] {
            assert!(opted_out(&[("LOCALHOST_TELEMETRY", value)]), "LOCALHOST_TELEMETRY={value}");
            assert!(!opted_out(&[("DO_NOT_TRACK", value)]), "DO_NOT_TRACK={value}");
        }
        assert!(!opted_out(&[("LOCALHOST_TELEMETRY", "1")]));
        assert!(!opted_out(&[("DO_NOT_TRACK", "")]));
        assert!(opted_out(&[("LOCALHOST_TELEMETRY", "1"), ("DO_NOT_TRACK", "1")]));
    }

    #[test]
    fn the_host_defaults_to_posthog_us() {
        let destination = |host| Allowance::new(Some("phc_test"), host, true, &environment(&[])).destination;
        assert_eq!(destination(None).unwrap().1, DEFAULT_HOST);
        assert_eq!(destination(Some("")).unwrap().1, DEFAULT_HOST);
        assert_eq!(destination(Some("https://e.example.com")).unwrap().1, "https://e.example.com");
    }

    #[test]
    fn turning_it_off_stops_sending_and_is_remembered() {
        let root = tempfile::tempdir().unwrap();
        let (telemetry, requests) = telemetry(root.path(), allowed(), true);
        assert!(telemetry.is_enabled());
        telemetry.set_enabled(false).unwrap();
        assert_eq!(telemetry.send_app_open(), None);
        assert!(requests.lock().unwrap().is_empty());

        let (reopened, _) = self::telemetry(root.path(), allowed(), true);
        assert!(!reopened.is_enabled());
        assert!(!reopened.is_notice_pending());
    }

    #[test]
    fn sends_the_first_open_once_then_opens_with_one_id() {
        let root = tempfile::tempdir().unwrap();
        let (first, first_requests) = telemetry(root.path(), allowed(), true);
        assert_eq!(first.send_app_open(), Some(TelemetryEvent::FirstOpen));
        let (reopened, requests) = telemetry(root.path(), allowed(), true);
        assert_eq!(reopened.send_app_open(), Some(TelemetryEvent::Opened));
        assert_eq!(reopened.send_app_open(), Some(TelemetryEvent::Opened));

        let first_requests = first_requests.lock().unwrap();
        let requests = requests.lock().unwrap();
        let all: Vec<_> = first_requests.iter().chain(requests.iter()).collect();
        let events: Vec<_> = all.iter().map(|(_, body)| body["event"].as_str().unwrap()).collect();
        assert_eq!(events, ["app_first_open", "app_opened", "app_opened"]);
        assert!(all.iter().all(|(_, body)| body["distinct_id"] == all[0].1["distinct_id"]));
        assert_eq!(all[0].0, "https://us.i.posthog.com/i/v0/e/");
    }

    #[test]
    fn a_failed_first_open_is_sent_again() {
        let root = tempfile::tempdir().unwrap();
        let (failing, _) = telemetry(root.path(), allowed(), false);
        assert_eq!(failing.send_app_open(), None);
        let (telemetry, requests) = telemetry(root.path(), allowed(), true);
        assert_eq!(telemetry.send_app_open(), Some(TelemetryEvent::FirstOpen));
        assert_eq!(requests.lock().unwrap().len(), 1);
    }

    #[test]
    fn the_payload_has_only_the_allowed_fields() {
        let root = tempfile::tempdir().unwrap();
        let (telemetry, requests) = telemetry(root.path(), allowed(), true);
        telemetry.send_app_open();
        let requests = requests.lock().unwrap();
        let body = requests[0].1.as_object().unwrap();

        let mut envelope: Vec<_> = body.keys().map(String::as_str).collect();
        envelope.sort();
        assert_eq!(envelope, ["api_key", "distinct_id", "event", "properties"]);
        let properties = body["properties"].as_object().unwrap();
        let mut names: Vec<_> = properties.keys().map(String::as_str).collect();
        names.sort();
        assert_eq!(
            names,
            ["$geoip_disable", "$process_person_profile", "app_version", "arch", "install_channel", "os", "product"]
        );
        assert_eq!(properties["$process_person_profile"], false);
        assert_eq!(properties["product"], "localhost-app");
        assert_eq!(properties["install_channel"], "dmg");
        assert!(["macos", "windows", "linux"].contains(&properties["os"].as_str().unwrap()));
        assert!(body["distinct_id"].as_str().unwrap().parse::<Uuid>().is_ok());
        // Nothing path-like or personal slips in.
        let text = serde_json::to_string(body).unwrap();
        for leak in [root.path().to_string_lossy().as_ref(), "/Users/", "/home/", "\\\\"] {
            assert!(!text.contains(leak), "{leak} in {text}");
        }
    }

    #[test]
    fn the_notice_shows_once_and_only_when_it_would_send() {
        let root = tempfile::tempdir().unwrap();
        let (telemetry, _) = telemetry(root.path(), allowed(), true);
        assert!(telemetry.is_notice_pending());
        telemetry.acknowledge_notice();
        assert!(!telemetry.is_notice_pending());
        assert!(!self::telemetry(root.path(), allowed(), true).0.is_notice_pending());

        let other = tempfile::tempdir().unwrap();
        let off = Allowance::new(Some("phc_test"), None, true, &environment(&[("DO_NOT_TRACK", "1")]));
        assert!(!self::telemetry(other.path(), off, true).0.is_notice_pending());
    }

    #[test]
    fn an_unreadable_file_is_left_alone_and_nothing_is_sent() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(FILE_NAME);
        fs::write(&path, "{ not json").unwrap();
        let (telemetry, requests) = telemetry(root.path(), allowed(), true);
        assert!(!telemetry.is_enabled());
        assert!(!telemetry.is_notice_pending());
        telemetry.set_enabled(true).unwrap();
        assert_eq!(telemetry.send_app_open(), None);
        assert!(requests.lock().unwrap().is_empty());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ not json");
    }

    #[test]
    fn detects_install_channels() {
        let none = |_: &str| false;
        let nothing = |_: &Path| false;
        if cfg!(target_os = "macos") {
            let app = Path::new("/Applications/localhost.app/Contents/MacOS/localhost");
            let caskroom = |path: &Path| path == Path::new("/opt/homebrew/Caskroom/localhost");
            assert_eq!(InstallChannel::detect_from(app, &none, &caskroom), InstallChannel::Brew);
            assert_eq!(InstallChannel::detect_from(app, &none, &nothing), InstallChannel::Dmg);
            let built = Path::new("/Users/me/localhost/core/target/release/localhost-app");
            assert_eq!(InstallChannel::detect_from(built, &none, &nothing), InstallChannel::Source);
        } else if cfg!(target_os = "linux") {
            let appimage = |name: &str| name == "APPIMAGE";
            let installed = Path::new("/usr/bin/localhost-app");
            assert_eq!(InstallChannel::detect_from(installed, &appimage, &nothing), InstallChannel::AppImage);
            let dpkg = |path: &Path| path == Path::new("/var/lib/dpkg/info/localhost.list");
            assert_eq!(InstallChannel::detect_from(installed, &none, &dpkg), InstallChannel::Deb);
            let rpm = |path: &Path| path == Path::new("/var/lib/rpm");
            assert_eq!(InstallChannel::detect_from(installed, &none, &rpm), InstallChannel::Rpm);
            assert_eq!(InstallChannel::detect_from(installed, &none, &nothing), InstallChannel::Unknown);
            let built = Path::new("/home/me/localhost/target/release/localhost-app");
            assert_eq!(InstallChannel::detect_from(built, &none, &nothing), InstallChannel::Source);
        } else if cfg!(windows) {
            let installed = Path::new(r"C:\Users\me\AppData\Local\localhost\localhost-app.exe");
            let uninstaller = |path: &Path| path == Path::new(r"C:\Users\me\AppData\Local\localhost\uninstall.exe");
            assert_eq!(InstallChannel::detect_from(installed, &none, &uninstaller), InstallChannel::Exe);
            assert_eq!(InstallChannel::detect_from(installed, &none, &nothing), InstallChannel::Source);
        }
    }
}
