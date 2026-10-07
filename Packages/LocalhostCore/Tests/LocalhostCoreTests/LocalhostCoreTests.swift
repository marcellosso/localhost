import Testing
@testable import LocalhostCore

@Test func `core exposes a version`() {
    #expect(!LocalhostCore.version.isEmpty)
}
