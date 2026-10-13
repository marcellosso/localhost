# WSL Projects are run by a core inside the distro

On Windows a Project has an Execution Target: this machine, or a WSL distro. Many Windows developers keep their repositories and toolchains inside WSL, where a Windows process can only start `wsl.exe` and sees none of the process tree, signals or listening sockets behind it. So for a WSL Project the Windows app starts the Linux build of the core inside the distro (`wsl.exe -d <distro> -- localhostd --stdio`) and drives it over that pipe. Process management, Listener discovery, Orphan records and detection all happen in Linux, with the same code as on Linux. Wrapping each command in `wsl.exe` was rejected because Stop, Ports and Orphans would not work.

## Consequences

- The Windows app copies `localhostd` into the distro (under `~/.local/share/localhost/`) and keeps it at the app's version.
- Talking over the process's stdio needs no socket, port or firewall rule. When the pipe closes (the app quits or crashes), `localhostd` stops its Services, as the in-process core does (ADR-0001).
- Project Configuration and Secrets stay on Windows. A Run's environment, Secret values included, is sent to `localhostd` at start and never stored in the distro.
- Ports reach the Windows browser through WSL's localhost forwarding. The Ports view shows the distro's Listeners, grouped under it.
- The same mechanism can later carry other targets (a remote machine, a dev container, #13).
