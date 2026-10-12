import Darwin
import Foundation

/// A process listening on one or more TCP ports on this machine, whoever
/// started it (see `GLOSSARY.md`).
public struct Listener: Identifiable, Sendable, Hashable, Codable {
    public var id: pid_t { pid }
    public let pid: pid_t
    public let processGroupID: pid_t
    /// The executable's name, e.g. `node`.
    public let name: String
    /// The full command line, e.g. `python3 -m http.server 3000`.
    public let command: String
    /// The executable's path, if it could be read.
    public let executablePath: String?
    /// The TCP ports it listens on, ascending.
    public let ports: [Int]
    /// Its current working directory, if it could be read.
    public let workingDirectory: String?
    public let owner: ListenerOwner
    /// The label of the launchd job it belongs to, if any (e.g. a Homebrew
    /// service or an app). localhost never stops those.
    public let launchdLabel: String?
    public let kind: ListenerKind
    let startTime: ProcessStartTime

    /// Whether it belongs to no Run and no Project.
    public var isUnmanaged: Bool { owner == .unmanaged }
}

/// What kind of program a Listener is, so development servers can be told
/// apart from the apps and system services that also listen on ports.
public enum ListenerKind: String, Sendable, Hashable, Codable {
    /// A development server or tool: anything not recognised as below, and
    /// always a Listener of a Run or inside a Project.
    case development
    /// Docker's backend, which listens on the ports of running containers.
    case docker
    /// A Mac app (or a helper inside one).
    case app
    /// Part of macOS.
    case system

    /// Classifies by owner, executable path and launchd label.
    static func of(executablePath: String?, launchdLabel: String?, owner: ListenerOwner) -> ListenerKind {
        if owner != .unmanaged { return .development }
        let path = executablePath ?? ""
        if path.contains("/Docker.app/"), (path as NSString).lastPathComponent.hasPrefix("com.docker.") { return .docker }
        let systemFolders = ["/System/", "/usr/libexec/", "/usr/sbin/", "/sbin/", "/Library/Apple/"]
        if systemFolders.contains(where: path.hasPrefix) || launchdLabel?.hasPrefix("com.apple.") == true { return .system }
        // Interpreters like Homebrew's Python run from an `.app` inside their framework.
        if (path.contains(".app/") && !path.contains(".framework/")) || launchdLabel?.hasPrefix("application.") == true {
            return .app
        }
        return .development
    }
}

/// Who a Listener is attributed to.
public enum ListenerOwner: Sendable, Hashable, Codable {
    /// A process of a Run localhost started.
    case run(serviceID: UUID, runID: UUID)
    /// Not started by localhost, but its working directory is inside the Project's folder.
    case project(UUID)
    case unmanaged
}

/// Finds Listeners through libproc, without attributing them.
enum ListenerScanner {
    /// A listening process as the kernel reports it.
    struct Process: Sendable {
        let pid: pid_t
        let processGroupID: pid_t
        let sessionID: pid_t
        let name: String
        let command: String
        let executablePath: String?
        let ports: [Int]
        let workingDirectory: String?
        let startTime: ProcessStartTime
    }

    /// Every process of this user listening on a TCP port. Processes of other
    /// users (e.g. root) can't be inspected and are left out.
    @concurrent
    static func scan() async -> [Process] {
        allPIDs().compactMap { pid in
            let ports = listeningPorts(of: pid)
            guard !ports.isEmpty, let info = bsdInfo(of: pid) else { return nil }
            let name = withUnsafeBytes(of: info.pbi_name) { String(decoding: $0.prefix { $0 != 0 }, as: UTF8.self) }
            let shortName = withUnsafeBytes(of: info.pbi_comm) { String(decoding: $0.prefix { $0 != 0 }, as: UTF8.self) }
            return Process(
                pid: pid,
                processGroupID: pid_t(info.pbi_pgid),
                sessionID: getsid(pid),
                name: name.isEmpty ? shortName : name,
                command: commandLine(of: pid) ?? shortName,
                executablePath: executablePath(of: pid),
                ports: ports,
                workingDirectory: workingDirectory(of: pid),
                startTime: ProcessStartTime(seconds: info.pbi_start_tvsec, microseconds: info.pbi_start_tvusec)
            )
        }
    }

    /// `path` with symlinks resolved, as the kernel reports working directories.
    static func resolvedPath(_ path: String) -> String {
        guard let resolved = realpath(path, nil) else { return path }
        defer { free(resolved) }
        return String(cString: resolved)
    }

    static func allPIDs() -> [pid_t] {
        let capacity = proc_listpids(UInt32(PROC_ALL_PIDS), 0, nil, 0)
        guard capacity > 0 else { return [] }
        // Room for processes started between the two calls.
        var pids = [pid_t](repeating: 0, count: Int(capacity) / MemoryLayout<pid_t>.size + 64)
        let size = proc_listpids(UInt32(PROC_ALL_PIDS), 0, &pids, Int32(pids.count * MemoryLayout<pid_t>.size))
        return pids.prefix(Int(max(size, 0)) / MemoryLayout<pid_t>.size).filter { $0 > 0 }
    }

    static func bsdInfo(of pid: pid_t) -> proc_bsdinfo? {
        var info = proc_bsdinfo()
        let size = Int32(MemoryLayout<proc_bsdinfo>.size)
        return proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, size) == size ? info : nil
    }

    /// The parent of every process, for finding descendants.
    static func parents() -> [pid_t: pid_t] {
        var parents: [pid_t: pid_t] = [:]
        for pid in allPIDs() {
            if let info = bsdInfo(of: pid) { parents[pid] = pid_t(info.pbi_ppid) }
        }
        return parents
    }

    private static func listeningPorts(of pid: pid_t) -> [Int] {
        let size = proc_pidinfo(pid, PROC_PIDLISTFDS, 0, nil, 0)
        guard size > 0 else { return [] }
        var fds = [proc_fdinfo](repeating: proc_fdinfo(), count: Int(size) / MemoryLayout<proc_fdinfo>.size)
        let filled = proc_pidinfo(pid, PROC_PIDLISTFDS, 0, &fds, size)
        guard filled > 0 else { return [] }
        var ports: Set<Int> = []
        for fd in fds.prefix(Int(filled) / MemoryLayout<proc_fdinfo>.size) where fd.proc_fdtype == PROX_FDTYPE_SOCKET {
            var info = socket_fdinfo()
            let infoSize = Int32(MemoryLayout<socket_fdinfo>.size)
            guard proc_pidfdinfo(pid, fd.proc_fd, PROC_PIDFDSOCKETINFO, &info, infoSize) == infoSize,
                  info.psi.soi_kind == SOCKINFO_TCP,
                  info.psi.soi_proto.pri_tcp.tcpsi_state == TSI_S_LISTEN
            else { continue }
            let port = UInt16(bigEndian: UInt16(truncatingIfNeeded: info.psi.soi_proto.pri_tcp.tcpsi_ini.insi_lport))
            if port != 0 { ports.insert(Int(port)) }
        }
        return ports.sorted()
    }

    private static func executablePath(of pid: pid_t) -> String? {
        // PROC_PIDPATHINFO_MAXSIZE, which Swift does not import.
        var buffer = [CChar](repeating: 0, count: 4 * Int(MAXPATHLEN))
        guard proc_pidpath(pid, &buffer, UInt32(buffer.count)) > 0 else { return nil }
        return String(cString: buffer)
    }

    private static func workingDirectory(of pid: pid_t) -> String? {
        var info = proc_vnodepathinfo()
        let size = Int32(MemoryLayout<proc_vnodepathinfo>.size)
        guard proc_pidinfo(pid, PROC_PIDVNODEPATHINFO, 0, &info, size) == size else { return nil }
        let path = withUnsafeBytes(of: info.pvi_cdir.vip_path) { String(decoding: $0.prefix { $0 != 0 }, as: UTF8.self) }
        return path.isEmpty ? nil : path
    }

    /// The process's arguments joined by spaces, from `KERN_PROCARGS2`:
    /// argc, the executable path, padding, then the arguments.
    private static func commandLine(of pid: pid_t) -> String? {
        var mib: [Int32] = [CTL_KERN, KERN_PROCARGS2, pid]
        var size = 0
        guard sysctl(&mib, 3, nil, &size, nil, 0) == 0, size > MemoryLayout<Int32>.size else { return nil }
        var buffer = [UInt8](repeating: 0, count: size)
        guard sysctl(&mib, 3, &buffer, &size, nil, 0) == 0 else { return nil }
        let argc = buffer.withUnsafeBytes { $0.loadUnaligned(as: Int32.self) }
        var index = MemoryLayout<Int32>.size
        // Skip the executable path and the NULs after it.
        while index < size, buffer[index] != 0 { index += 1 }
        while index < size, buffer[index] == 0 { index += 1 }
        var arguments: [String] = []
        while arguments.count < argc, index < size {
            let end = buffer[index..<size].firstIndex(of: 0) ?? size
            arguments.append(String(decoding: buffer[index..<end], as: UTF8.self))
            index = end + 1
        }
        return arguments.isEmpty ? nil : arguments.joined(separator: " ")
    }
}

/// The pids of the user's launchd jobs that are running (Homebrew services,
/// apps, agents), by label.
enum LaunchdJobs {
    @concurrent
    static func running() async -> [pid_t: String] {
        let process = Foundation.Process()
        process.executableURL = URL(filePath: "/bin/launchctl")
        process.arguments = ["list"]
        let output = Pipe()
        process.standardOutput = output
        process.standardError = FileHandle.nullDevice
        do { try process.run() } catch { return [:] }
        let data = output.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return parse(String(decoding: data, as: UTF8.self))
    }

    /// Parses `launchctl list`: `PID\tStatus\tLabel` lines, PID `-` when not running.
    static func parse(_ output: String) -> [pid_t: String] {
        var jobs: [pid_t: String] = [:]
        for line in output.split(separator: "\n") {
            let fields = line.split(separator: "\t", maxSplits: 2)
            guard fields.count == 3, let pid = pid_t(fields[0]) else { continue }
            jobs[pid] = String(fields[2])
        }
        return jobs
    }
}

/// Stops a Listener localhost didn't start: SIGINT, then SIGTERM, then SIGKILL.
enum ListenerStopper {
    /// Signals the Listener's process group, unless the group's leader is a
    /// session leader (e.g. a terminal's shell) or the group is localhost's
    /// own: then only the Listener's process and its descendants.
    static func stop(_ listener: Listener, policy: StopPolicy) async {
        let group = listener.processGroupID
        var target: Target = group > 1 && group != getpgrp() && getsid(group) != group
            ? .group(group)
            : .processes([listener.pid: listener.startTime])
        let steps: [(Int32, Duration)] = [
            (SIGINT, policy.interruptTimeout),
            (SIGTERM, policy.terminateTimeout),
            (SIGKILL, .seconds(5)),
        ]
        for (signal, timeout) in steps {
            target.includeDescendants()
            guard target.isAlive else { return }
            target.send(signal)
            let deadline = SuspendingClock.now + timeout
            while target.isAlive, SuspendingClock.now < deadline {
                try? await Task.sleep(for: .milliseconds(20))
            }
        }
    }

    private enum Target {
        case group(pid_t)
        /// Processes identified by pid and start time, so a reused pid is never signalled.
        case processes([pid_t: ProcessStartTime])

        var isAlive: Bool {
            switch self {
            case .group(let group): kill(-group, 0) == 0 || errno == EPERM
            case .processes(let processes): processes.contains { ProcessStartTime.of($0.key) == $0.value }
            }
        }

        func send(_ signal: Int32) {
            switch self {
            case .group(let group):
                kill(-group, signal)
            case .processes(let processes):
                for (pid, startTime) in processes where ProcessStartTime.of(pid) == startTime {
                    kill(pid, signal)
                }
            }
        }

        /// Adds the current descendants of the tracked processes that are still alive.
        mutating func includeDescendants() {
            guard case .processes(var processes) = self else { return }
            var children: [pid_t: [pid_t]] = [:]
            for (pid, parent) in ListenerScanner.parents() { children[parent, default: []].append(pid) }
            var queue = processes.filter { ProcessStartTime.of($0.key) == $0.value }.map(\.key)
            while let pid = queue.popLast() {
                for child in children[pid] ?? [] where processes[child] == nil {
                    guard let startTime = ProcessStartTime.of(child) else { continue }
                    processes[child] = startTime
                    queue.append(child)
                }
            }
            self = .processes(processes)
        }
    }
}
