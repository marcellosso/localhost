import Darwin
import Testing
@testable import LocalhostCore

struct LocalhostErrorTests {
    @Test func `messages name what went wrong`() throws {
        #expect(LocalhostError.duplicateProjectKey(try Key("api")).message == "Another project already uses the key “api”.")
        #expect(LocalhostError.workingDirectoryMissing("/tmp/x").message == "The working directory /tmp/x does not exist.")
        #expect(LocalhostError.commandNotFound.message == "Command not found.")
        #expect(LocalhostError.launchFailed(errno: ENOMEM).message.contains("Cannot allocate memory"))
    }
}
