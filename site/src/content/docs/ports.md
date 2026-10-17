A Service's **Port** is the one TCP port it is expected to listen on. The command gets it as `$PORT` (`%PORT%` in cmd); localhost never rewrites command lines or adds framework flags.

## When a Service is Running

With a Port, a Service counts as Running once the port accepts connections. After 30 seconds it is Running anyway, with a warning that clears when the port opens. Without a Port, it is Running after about 1.5 seconds alive.

## When a port is taken

Before starting a Service with a Port, localhost checks who holds it. If something else does, the Service can start on the next free port (it gets that port as `$PORT` for this Run; the configured Port doesn't change), or you choose: stop the other process and start, start on another port, or cancel.

Settings turns the automatic move off; a Service's **Port can change** switch turns it off for that Service. A Service that others depend on never moves, and neither does one whose port is held by something in the same Project.

## Open in Browser

**Open in Browser** (⇧⌘O) opens the Service's URL with its port moved to the Run's, or `http://localhost:<port>`, or else the first local URL the Run printed.

## The Ports view

The **Ports** view (**⌘0**) lists every process listening on a TCP port and whom it belongs to: one of your Services, a process running inside one of your Projects' folders, or nothing localhost knows (other apps and system services are folded below). Select one and press ⌘. to stop it; localhost asks first for anything it didn't start.
