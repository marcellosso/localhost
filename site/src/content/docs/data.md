| | Configuration, logs and Run records | Secret values |
|---|---|---|
| macOS | `~/Library/Application Support/localhost` | login Keychain, service `io.github.marcellosso.localhost.secret` |
| Linux | `~/.local/share/localhost` (or `$XDG_DATA_HOME/localhost`) | Secret Service, attributes `service` = `io.github.marcellosso.localhost.secret` and `account` |
| Windows | `%APPDATA%\localhost`; a WSL Project's logs and Run records stay in its distro, under `~/.local/share/localhost/windows/` | Credential Manager, generic credentials `io.github.marcellosso.localhost.secret/<account>` |

Inside that folder:

- `projects/`: one JSON file per Project. A file that can't be read is renamed to `….unreadable` and reported at launch, never deleted.
- `logs/`: each Run's output, the last 5 Runs per Service.
- `runs/`: the Runs active right now, to find leftovers after a crash.

The Linux and Windows app keeps its settings in `~/.config/io.github.marcellosso.localhost.desktop` or `%APPDATA%\io.github.marcellosso.localhost.desktop`. Set `LOCALHOST_STORAGE_ROOT` to use another folder, e.g. to try a build without touching your data.

Removing a Service or a Project stops it first and deletes its logs. Your project's folder is never touched.
