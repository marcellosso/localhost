# A Run's CPU and memory are sampled on request, over its whole tree, with each platform's honest memory number

localhost shows what a Run's process tree uses (#94), so a busy file watcher or a slow leak can be traced to its Service. The core samples nothing on its own. `run_stats(service, previous)` measures the tree once and, given the previous sample of the same Run, returns its CPU use since then. Clients call it every ~1.5s only while a stats popover is open. Keeping a ring buffer from the start of every Run was rejected because it would mean sampling every running Service all the time for a view that is rarely open. The call is stateless, so it also works over the `localhostd` pipe (ADR-0008) and as a one-shot query for the v2 CLI/MCP, which takes two samples.

A Run's processes are those in its session or process group (as for Listeners) plus every descendant of its shell, so a child that `setsid`s or daemonizes without double-forking still counts. On native Windows they are the processes in the Run's Job Object (ADR-0011). Processes waiting to be reaped are left out.

Memory counts shared pages once wherever the platform allows:

| | Memory | CPU time |
|---|---|---|
| macOS | `ri_phys_footprint` from `proc_pid_rusage` (Activity Monitor's "Memory") | user + system time, plus children it has waited for |
| Linux, WSL | PSS from `/proc/<pid>/smaps_rollup`, RSS from `statm` when that can't be read | `utime + stime + cutime + cstime` |
| Windows | private bytes (`PrivateUsage`) | the job's total user + kernel time |

RSS everywhere was rejected because forked workers (node cluster, gunicorn) would be counted once per worker. Reading `smaps_rollup` is slower, but it only happens while someone is looking.

## Consequences

- CPU % is the CPU time used between two samples divided by the wall time between them, so 100% is one core. On macOS and Linux the sum covers live processes, and a process's time moves to its parent once the parent reaps it. A process that is reaped outside the tree takes its time with it, so the CPU % for that interval is clamped at zero. The job's total on Windows never goes down.
- The number shown as "Memory" means something slightly different on each platform. The UIs say what it is in a tooltip rather than pretending the numbers are the same.
- "Peak memory" is the highest sample since the popover opened, not the Run's lifetime peak.
- A double-forked daemon reparented to launchd or init is not counted. It is outside the Run, just as it is outside Stop.
- `localhostd` speaks protocol 2 because `RunStats` was added to it.
