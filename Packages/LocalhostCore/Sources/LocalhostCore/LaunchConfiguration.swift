import Foundation

/// Everything needed to launch one Run's process tree.
public struct LaunchConfiguration: Sendable, Hashable {
    public var commandLine: String
    public var workingDirectory: URL
    /// The complete environment of the Run (see `RunEnvironment.merged`).
    public var environment: [String: String]
    /// The shell the command line runs through, as `<shell> -l -c <commandLine>`.
    public var shell: String

    public init(
        commandLine: String,
        workingDirectory: URL,
        environment: [String: String],
        shell: String? = nil
    ) {
        self.commandLine = commandLine
        self.workingDirectory = workingDirectory
        self.environment = environment
        self.shell = shell ?? RunEnvironment.shell(in: environment)
    }
}

/// Builds the environment of a Run. Later layers override earlier ones:
/// base (the app's environment) → Service variables, plus output hints that
/// keep colour and unbuffered output when writing to pipes (ADR-0005).
///
/// The command runs through a login shell, so the user's login profile is
/// applied on top when the shell starts (ADR-0003).
public enum RunEnvironment {
    public static let outputHints = [
        "FORCE_COLOR": "1",
        "CLICOLOR_FORCE": "1",
        "PYTHONUNBUFFERED": "1",
    ]

    public static func merged(
        base: [String: String] = ProcessInfo.processInfo.environment,
        service: [String: String]
    ) -> [String: String] {
        // Loader variables injected into the app (e.g. by Xcode's debugger)
        // must not leak into the user's commands.
        base.filter { !$0.key.hasPrefix("DYLD_") && !$0.key.hasPrefix("__XPC_") }
            .merging(outputHints) { _, hint in hint }
            .merging(service) { _, service in service }
    }

    /// The user's shell: `$SHELL`, or zsh (the macOS default).
    public static func shell(in environment: [String: String]) -> String {
        guard let shell = environment["SHELL"], !shell.isEmpty else { return "/bin/zsh" }
        return shell
    }
}
