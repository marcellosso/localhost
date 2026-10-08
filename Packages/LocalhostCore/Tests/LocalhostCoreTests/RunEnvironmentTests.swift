import Testing
@testable import LocalhostCore

struct RunEnvironmentTests {
    @Test func `variables override the base environment`() {
        let environment = RunEnvironment.merged(
            base: ["PATH": "/usr/bin", "PORT": "1"],
            variables: ["PORT": "3000", "API_URL": "http://localhost:8000"]
        )
        #expect(environment["PATH"] == "/usr/bin")
        #expect(environment["PORT"] == "3000")
        #expect(environment["API_URL"] == "http://localhost:8000")
    }

    @Test func `adds the output hints`() {
        let environment = RunEnvironment.merged(base: [:], variables: [:])
        #expect(environment["FORCE_COLOR"] == "1")
        #expect(environment["CLICOLOR_FORCE"] == "1")
        #expect(environment["PYTHONUNBUFFERED"] == "1")
    }

    @Test func `a variable can override an output hint`() {
        let environment = RunEnvironment.merged(base: [:], variables: ["FORCE_COLOR": "0"])
        #expect(environment["FORCE_COLOR"] == "0")
    }

    @Test func `drops loader variables injected into the app`() {
        let environment = RunEnvironment.merged(
            base: ["DYLD_INSERT_LIBRARIES": "/x.dylib", "__XPC_DYLD_LIBRARY_PATH": "/y", "HOME": "/Users/dev"],
            variables: [:]
        )
        #expect(environment["DYLD_INSERT_LIBRARIES"] == nil)
        #expect(environment["__XPC_DYLD_LIBRARY_PATH"] == nil)
        #expect(environment["HOME"] == "/Users/dev")
    }

    @Test func `the shell comes from SHELL, falling back to zsh`() {
        #expect(RunEnvironment.shell(in: ["SHELL": "/opt/homebrew/bin/fish"]) == "/opt/homebrew/bin/fish")
        #expect(RunEnvironment.shell(in: [:]) == "/bin/zsh")
        #expect(RunEnvironment.shell(in: ["SHELL": ""]) == "/bin/zsh")
    }
}
