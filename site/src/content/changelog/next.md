---
version: next
title: Windows, WSL and live CPU and memory
date: 2026-10-17
released: false
---

On `main`, in the next release.

- **Windows**: the Linux and Windows app as a per-user installer. Services run in `cmd.exe` with your user environment read fresh at each start, inside a Job Object, and stop with Ctrl-Break.
- **WSL**: a Project can run inside a WSL distro, through a small `localhostd` the app copies into it. Commands, detection and the Ports view run in the distro.
- **Stats**: a running Service's CPU and memory, sampled only while its popover is open.
- **Keyboard Shortcuts** sheet on ⌘/ and Ctrl+/.
