import Darwin
import Foundation
import Testing
@testable import LocalhostCore

/// These tests launch real processes. Each command line is a fixture.
struct ProcessTreeTests {
    /// Short timeouts so escalation tests stay fast; the escalation order is what matters.
    let quickStop = StopPolicy(interruptTimeout: .milliseconds(500), terminateTimeout: .milliseconds(500))
    /// For tests expecting a stop to end before escalating: the timeouts only bound a failure, and a
    /// busy machine (CI, the whole suite in parallel) can take longer than `quickStop` allows.
    let patientStop = StopPolicy(interruptTimeout: .seconds(30), terminateTimeout: .seconds(30))

    func launch(
        _ commandLine: String,
        in directory: String = NSTemporaryDirectory(),
        environment: [String: String] = ProcessInfo.processInfo.environment,
        shell: String = "/bin/sh",
        stopPolicy: StopPolicy? = nil
    ) throws -> ProcessTree {
        try ProcessTree.launch(
            LaunchConfiguration(
                commandLine: commandLine,
                workingDirectory: URL(filePath: directory),
                environment: environment,
                shell: shell
            ),
            stopPolicy: stopPolicy ?? quickStop
        )
    }

    /// Reads output lines until `count` have arrived.
    func firstLines(_ count: Int, of tree: ProcessTree) async -> [OutputLine] {
        var lines: [OutputLine] = []
        for await batch in tree.output {
            lines += batch
            if lines.count >= count { break }
        }
        return Array(lines.prefix(count))
    }

    func allLines(of tree: ProcessTree) async -> [OutputLine] {
        var lines: [OutputLine] = []
        for await batch in tree.output { lines += batch }
        return lines
    }

    // MARK: Exit codes

    @Test(arguments: [
        ("exit 0", Termination.exited(code: 0)),
        ("exit 1", Termination.exited(code: 1)),
        ("exit 42", Termination.exited(code: 42)),
        ("kill -TERM $$", Termination.signaled(SIGTERM)),
    ])
    func `captures how the command ended`(commandLine: String, expected: Termination) async throws {
        let exit = try await launch(commandLine).waitForExit()
        #expect(exit.termination == expected)
        #expect(exit.strongestSignalSent == nil)
    }

    @Test func `an unknown command ends with 127, mapped to commandNotFound`() async throws {
        let tree = try launch("definitely-not-a-command-\(UUID().uuidString.prefix(8))")
        let lines = await allLines(of: tree)
        let exit = await tree.waitForExit()

        #expect(exit.termination == .exited(code: 127))
        #expect(exit.termination.launchError == .commandNotFound)
        #expect(lines.contains { $0.channel == .stderr && $0.text.contains("not found") })
    }

    @Test func `a command that is not executable is mapped to commandNotExecutable`() async throws {
        let directory = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        try Data("echo hi\n".utf8).write(to: directory.appending(path: "script.sh"))

        let exit = try await launch("./script.sh", in: directory.path).waitForExit()
        #expect(exit.termination.launchError == .commandNotExecutable)
    }

    @Test func `success and ordinary failures are not launch errors`() {
        #expect(Termination.exited(code: 0).launchError == nil)
        #expect(Termination.exited(code: 1).launchError == nil)
        #expect(Termination.signaled(SIGKILL).launchError == nil)
    }

    // MARK: Launch failures

    @Test func `a missing working directory fails the launch`() throws {
        let missing = "/tmp/localhost-missing-\(UUID().uuidString)"
        #expect(throws: LocalhostError.workingDirectoryMissing(missing)) {
            try launch("true", in: missing)
        }
    }

    @Test func `a missing shell fails the launch`() throws {
        #expect(throws: LocalhostError.shellNotFound("/no/such/shell")) {
            try launch("true", shell: "/no/such/shell")
        }
    }

    // MARK: Environment and working directory

    @Test func `runs in the working directory with the given environment`() async throws {
        let tree = try launch(
            #"pwd -P; echo "$GREETING""#,
            in: "/usr/bin",
            environment: ["GREETING": "hello", "HOME": NSHomeDirectory(), "PATH": "/usr/bin:/bin"]
        )
        let lines = await allLines(of: tree)
        #expect(lines.filter { $0.channel == .stdout }.map(\.text) == ["/usr/bin", "hello"])
    }

    @Test func `runs the command line through a login shell`() async throws {
        let tree = try launch("[[ -o login ]] && echo login || echo not-login", shell: "/bin/zsh")
        let lines = await allLines(of: tree)
        #expect(lines.last?.text == "login")
    }

    @Test func `output that isn't taken makes the command wait, without losing lines`() async throws {
        let marker = NSTemporaryDirectory() + "localhost-backpressure-\(UUID().uuidString)"
        defer { try? FileManager.default.removeItem(atPath: marker) }
        // 11 MB, far more than the pipes and the pending batches hold.
        let tree = try launch("yes 0123456789 | head -n 1000000; echo done; touch '\(marker)'")

        try await Task.sleep(for: .seconds(1))
        #expect(!FileManager.default.fileExists(atPath: marker))

        let lines = await allLines(of: tree)
        #expect(lines.count == 1_000_001)
        #expect(lines.dropLast().allSatisfy { $0.text == "0123456789" && $0.channel == .stdout })
        #expect(lines.last?.text == "done")
        #expect(FileManager.default.fileExists(atPath: marker))
    }

    @Test func `stdin is empty`() async throws {
        let tree = try launch("cat; echo done")
        #expect(await allLines(of: tree).map(\.text) == ["done"])
    }

    // MARK: Output

    @Test func `streams stdout and stderr as separate channels, line by line`() async throws {
        let tree = try launch("echo out1; echo err1 >&2; echo out2")
        let lines = await allLines(of: tree)

        #expect(lines.filter { $0.channel == .stdout }.map(\.text) == ["out1", "out2"])
        #expect(lines.filter { $0.channel == .stderr }.map(\.text) == ["err1"])
    }

    @Test func `delivers lines as they are written, not at exit`() async throws {
        let tree = try launch("echo first; sleep 1000")
        let lines = await firstLines(1, of: tree)
        #expect(lines.map(\.text) == ["first"])
        await tree.stop()
    }

    @Test func `keeps empty lines, strips carriage returns, and flushes a final partial line`() async throws {
        let tree = try launch(#"printf 'a\r\n\nb\nno newline'"#)
        #expect(await allLines(of: tree).map(\.text) == ["a", "", "b", "no newline"])
    }

    @Test func `replaces invalid UTF-8 instead of dropping the line`() async throws {
        let tree = try launch(#"printf 'caf\351\n'"#)
        #expect(await allLines(of: tree).map(\.text) == ["caf\u{FFFD}"])
    }

    @Test func `a line longer than the limit is split without breaking a character`() async throws {
        // 3-byte characters, so the 64 KB limit falls inside one of them.
        let tree = try launch(#"awk 'BEGIN { s = ""; for (i = 0; i < 50000; i++) s = s "€"; print s }'"#)
        let pieces = await allLines(of: tree).map(\.text)

        #expect(pieces.count > 1)
        #expect(!pieces.joined().contains("\u{FFFD}"))
        #expect(pieces.joined() == String(repeating: "€", count: 50_000))
    }

    @Test func `a line is cut at the limit even when its end arrives in a later read`() async throws {
        // Exactly the limit, then (in another read) the rest of the line.
        let tree = try launch(#"head -c 65536 /dev/zero | tr '\0' x; sleep 0.2; printf 'yyyy\n'"#)
        let lines = await allLines(of: tree).map(\.text)

        #expect(lines == [String(repeating: "x", count: PipeReader.maxLineLength), "yyyy"])
    }

    @Test(arguments: [
        ([UInt8](repeating: 0x78, count: PipeReader.maxLineLength + 1), PipeReader.maxLineLength),
        // "€" is E2 82 AC: the limit falls after its first byte, so the piece ends before it.
        ([UInt8](repeating: 0x78, count: PipeReader.maxLineLength - 1) + [0xE2, 0x82, 0xAC], PipeReader.maxLineLength - 1),
        // Only continuation bytes: not UTF-8, cut at the limit.
        ([UInt8](repeating: 0x80, count: PipeReader.maxLineLength + 1), PipeReader.maxLineLength),
    ])
    func `long lines are cut at a character boundary`(bytes: [UInt8], end: Int) {
        #expect(PipeReader.pieceLength(of: bytes[...]) == end)
    }

    @Test func `timestamps each line`() async throws {
        let before = Date()
        let lines = await allLines(of: try launch("echo hi"))
        let line = try #require(lines.first)
        #expect(line.timestamp >= before && line.timestamp <= Date())
    }

    // MARK: Stop

    @Test func `stop kills the whole tree, including grandchildren`() async throws {
        let tree = try launch("sleep 1000 & echo $!; sleep 1000 & echo $!; wait")
        let grandchildren = await firstLines(2, of: tree).compactMap { pid_t($0.text) }
        #expect(grandchildren.count == 2)
        #expect(grandchildren.allSatisfy(isAlive))

        let exit = await tree.stop()

        #expect(!processGroupExists(tree.pid))
        #expect(!grandchildren.contains(where: isAlive))
        #expect(exit.strongestSignalSent != nil)
    }

    @Test func `a command that handles SIGINT stops gracefully without escalation`() async throws {
        let tree = try launch(
            "trap 'echo bye; exit 0' INT; echo ready; while :; do sleep 0.05; done", stopPolicy: patientStop
        )
        _ = await firstLines(1, of: tree)

        let exit = await tree.stop()

        #expect(exit.strongestSignalSent == SIGINT)
        #expect(exit.termination == .exited(code: 0))
        #expect(!processGroupExists(tree.pid))
    }

    @Test func `a command that ignores SIGINT and SIGTERM is escalated to SIGKILL`() async throws {
        let tree = try launch("trap '' INT TERM; echo ready; sleep 1000 & wait")
        _ = await firstLines(1, of: tree)

        let started = ContinuousClock.now
        let exit = await tree.stop()

        #expect(exit.strongestSignalSent == SIGKILL)
        #expect(exit.wasForceKilled)
        #expect(exit.termination == .signaled(SIGKILL))
        #expect(!processGroupExists(tree.pid))
        // SIGINT and SIGTERM each got their full timeout before SIGKILL.
        #expect(ContinuousClock.now - started >= .milliseconds(1000))
    }

    @Test func `the default policy waits 5s after SIGINT and 3s after SIGTERM`() {
        #expect(StopPolicy.default == StopPolicy(interruptTimeout: .seconds(5), terminateTimeout: .seconds(3)))
    }

    @Test func `stop is idempotent and safe to call concurrently`() async throws {
        let tree = try launch("echo ready; sleep 1000")
        _ = await firstLines(1, of: tree)

        async let first = tree.stop()
        async let second = tree.stop()
        let (a, b) = await (first, second)
        let third = await tree.stop()

        #expect(a == b)
        #expect(b == third)
        #expect(!processGroupExists(tree.pid))
    }

    @Test func `stopping a tree that already ended returns its exit without signalling`() async throws {
        let tree = try launch("exit 3")
        let exit = await tree.waitForExit()
        #expect(await tree.stop() == exit)
        #expect(exit.strongestSignalSent == nil)
    }

    @Test func `processes left behind when the shell exits are cleaned up, starting with SIGTERM`() async throws {
        let tree = try launch("sleep 1000 & echo $!; exit 0", stopPolicy: patientStop)
        let leftover = try #require(await firstLines(1, of: tree).first.flatMap { pid_t($0.text) })

        let exit = await tree.waitForExit()

        #expect(exit.termination == .exited(code: 0))
        #expect(exit.strongestSignalSent == SIGTERM)
        #expect(!isAlive(leftover))
        #expect(!processGroupExists(tree.pid))
    }

    @Test func `reports the shell's exit before leftovers are cleaned up`() async throws {
        let tree = try launch(leftoverIgnoringSignals(thenParent: "exit 1"))

        let termination = await tree.waitForShellExit()

        #expect(termination == .exited(code: 1))
        #expect(processGroupExists(tree.pid))
        let exit = await tree.waitForExit()
        #expect(exit.termination == termination)
        #expect(exit.wasForceKilled)
    }

    @Test func `output ends once the tree has ended`() async throws {
        let tree = try launch("echo ready; sleep 1000")
        _ = await firstLines(1, of: tree)
        await tree.stop()

        // The stream finishes rather than hanging.
        #expect(await allLines(of: tree).isEmpty)
    }

    @Test func `the process group is the launched shell's own`() async throws {
        let tree = try launch("echo ready; sleep 1000")
        _ = await firstLines(1, of: tree)
        #expect(getpgid(tree.pid) == tree.pid)
        #expect(getsid(tree.pid) == tree.pid)
        #expect(getpgid(tree.pid) != getpgrp())
        await tree.stop()
    }
}

// MARK: Helpers

/// A command line whose shell leaves behind a subshell that ignores SIGINT
/// and SIGTERM. The parent runs `thenParent` only once the subshell's trap is
/// in place, so the fixture can't race the signals.
func leftoverIgnoringSignals(thenParent parentCommands: String) -> String {
    let marker = NSTemporaryDirectory() + "localhost-ready-\(UUID().uuidString)"
    return """
    (trap '' INT TERM; touch '\(marker)'; sleep 1000) & \
    while [ ! -e '\(marker)' ]; do sleep 0.01; done; rm '\(marker)'; \(parentCommands)
    """
}

func isAlive(_ pid: pid_t) -> Bool {
    kill(pid, 0) == 0 || errno == EPERM
}

func processGroupExists(_ pgid: pid_t) -> Bool {
    kill(-pgid, 0) == 0 || errno == EPERM
}
