import Testing
@testable import LocalhostCore

struct KeyTests {
    @Test(arguments: ["api", "my-saas", "web2", "a-b-c", "0", String(repeating: "a", count: 64)])
    func `accepts slugs`(raw: String) throws {
        #expect(try Key(raw).rawValue == raw)
    }

    @Test(arguments: ["", "API", "my saas", "-api", "api-", "my--saas", "api_v2", "café", String(repeating: "a", count: 65)])
    func `rejects anything that is not a slug`(raw: String) {
        #expect(throws: LocalhostError.invalidKey(raw)) { try Key(raw) }
    }

    @Test(arguments: [
        ("My SaaS", "my-saas"),
        ("  Café API  ", "cafe-api"),
        ("web_frontend (v2)", "web-frontend-v2"),
        ("Docker: Postgres!", "docker-postgres"),
    ])
    func `suggests a key from a name`(name: String, expected: String) {
        #expect(Key(suggestedFrom: name)?.rawValue == expected)
    }

    @Test func `suggestion is nil when the name has no usable characters`() {
        #expect(Key(suggestedFrom: "!!! ") == nil)
    }

    @Test func `suggestion is cut to the maximum length`() throws {
        let key = try #require(Key(suggestedFrom: String(repeating: "ab ", count: 40)))
        #expect(key.rawValue.count <= Key.maxLength)
        #expect(!key.rawValue.hasSuffix("-"))
    }
}
