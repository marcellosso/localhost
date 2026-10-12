import Foundation
import Testing
@testable import LocalhostCore

/// Project files: writing, reading with errors that name the line and field,
/// and importing into the library (Replace / Keep both).
final class ProjectDocumentTests {
    let root = FileManager.default.temporaryDirectory
        .appending(path: "LocalhostCoreTests-\(UUID().uuidString)", directoryHint: .isDirectory)
    let secrets = InMemorySecretStore()
    let home = FileManager.default.homeDirectoryForCurrentUser.path

    deinit {
        try? FileManager.default.removeItem(at: root)
    }

    func makeLibrary() throws -> ProjectLibrary {
        try ProjectLibrary(storageRoot: root, secrets: secrets)
    }

    /// A Project using every field the file has: Services with Dependencies,
    /// variables and Secrets on every layer, two Profiles, one active.
    func makeFullProject(in library: ProjectLibrary, key: String = "my-saas") async throws -> Project {
        let project = try await library.addProject(name: "My SaaS", key: Key(key), path: "\(home)/Code/my-saas", origin: .mainWindow)
        try await library.editProject(
            id: project.id, isFavorite: true,
            variables: [.plain("LOG_LEVEL", "debug"), .secret("SENTRY_DSN", "https://sentry.example/1")],
            origin: .mainWindow
        )
        let database = try await library.addService(
            toProject: project.id, name: "Database", key: Key("database"),
            commandLine: "docker compose up postgres", port: 5432, portCanChange: false, origin: .mainWindow
        )
        let api = try await library.addService(
            toProject: project.id, name: "API", key: Key("api"), workingDirectory: "./api/",
            commandLine: "uv run fastapi dev --port $PORT",
            variables: [.plain("DATABASE_URL", "postgres://localhost:5432/app"), .secret("STRIPE_KEY", "sk_test_123")],
            dependencies: [database.id], port: 8000, origin: .mainWindow
        )
        try await library.addService(
            toProject: project.id, name: "Web", key: Key("web"), workingDirectory: "web",
            commandLine: "pnpm dev", dependencies: [api.id], isExcludedFromStartAll: true,
            port: 3000, url: "http://localhost:3000/admin", origin: .mainWindow
        )
        let development = try await library.addProfile(
            toProject: project.id, name: "Development", variables: [.plain("NODE_ENV", "development")], origin: .mainWindow
        )
        try await library.addProfile(toProject: project.id, name: "Testing", origin: .mainWindow)
        try await library.setActiveProfile(development.id, inProject: project.id, origin: .mainWindow)
        return try await library.project(id: project.id)
    }

    // MARK: Writing

    @Test func `writes every field, with the home folder as a tilde and Secrets by name only`() async throws {
        let project = try await makeFullProject(in: makeLibrary())

        let yaml = ProjectDocument(project).yaml

        #expect(yaml == """
            localhost: 1
            name: My SaaS
            key: my-saas
            path: ~/Code/my-saas
            variables:
              LOG_LEVEL: debug
            secrets:
              - SENTRY_DSN
            services:
              database:
                name: Database
                command: docker compose up postgres
                cwd: .
                port: 5432
                port_can_change: false
              api:
                name: API
                command: uv run fastapi dev --port $PORT
                cwd: api
                port: 8000
                depends_on:
                  - database
                variables:
                  DATABASE_URL: postgres://localhost:5432/app
                secrets:
                  - STRIPE_KEY
              web:
                name: Web
                command: pnpm dev
                cwd: web
                port: 3000
                url: http://localhost:3000/admin
                start_all: false
                depends_on:
                  - api
            profiles:
              Development:
                variables:
                  NODE_ENV: development
              Testing: {}
            active_profile: Development

            """)
        #expect(!yaml.contains("sk_test_123"))
        #expect(!yaml.contains("sentry.example"))
    }

    @Test(arguments: [
        "", " leading", "trailing ", "true", "No", "null", "~", "3000", "1.5", "0x1F", "12:30", "-v", "[a]", "{}",
        "a: b", "a #b", "a#b", "ends:", "say \"hi\"", "it's", "back\\slash", "line\nbreak", "tab\there", "bell\u{7}",
        "ünïcødé ✓", "$PORT", "http://localhost:3000/a?b=c&d#e", "'quoted'", "\"quoted\"", "%x", "@x", "* x", "! x",
    ])
    func `writes values that read back as the same text`(value: String) throws {
        let yaml = "localhost: 1\nkey: \(YAMLScalar.write(value))\n"
        let root = try YAMLValue.parse(yaml)
        guard case .mapping(let entries) = root.kind else { Issue.record("Not a mapping"); return }
        #expect(entries[1].value.kind == .scalar(value))
    }

    // MARK: Round trip

    @Test func `export, import, export gives the same file and the same Project but for identity and Secret values`() async throws {
        let original = try await makeFullProject(in: makeLibrary())
        let exported = ProjectDocument(original).yaml

        // Import on another Mac: a fresh library.
        let otherRoot = root.appending(path: "other")
        let other = try ProjectLibrary(storageRoot: otherRoot, secrets: InMemorySecretStore())
        let imported = try await other.importProject(ProjectDocument(yaml: exported), origin: .mainWindow)

        #expect(ProjectDocument(imported).yaml == exported)
        #expect(imported.id != original.id)
        #expect(!imported.isFavorite)
        #expect(ProjectDocument(imported) == ProjectDocument(original))
        let api = try #require(imported.services.first { $0.key.rawValue == "api" })
        #expect(api.secretNames == ["STRIPE_KEY"])
        #expect(try await !other.isSecretSet(named: "STRIPE_KEY", in: .service(api.id)))
        #expect(imported.activeProfile?.name == "Development")
        #expect(imported.services.map { $0.dependencies.compactMap { imported.service(id: $0)?.key.rawValue } } == [[], ["database"], ["api"]])
    }

    // MARK: Reading

    @Test func `reads a hand-written file like the spec's example`() throws {
        let document = try ProjectDocument(yaml: """
            # Written by hand.
            localhost: 1
            name: "My SaaS"
            key: my-saas
            path: ~/Code/my-saas

            services:
              web:
                command: pnpm dev   # the Vite app
                cwd: ./web/
                port: "3000"
              api:
                command: 'uv run fastapi dev'
                cwd: ./api
                port: 8000
                depends_on: [database, database]
                variables:
                  DEBUG: true
                  EMPTY:
              database:
                command: docker compose up postgres
                port_can_change: false
            """)

        #expect(document.name == "My SaaS")
        #expect(document.path == "\(home)/Code/my-saas")
        #expect(document.services.map(\.name) == ["web", "api", "database"])
        #expect(document.services[0].workingDirectory == "web")
        #expect(document.services[0].port == 3000)
        #expect(document.services[1].dependencies.map(\.rawValue) == ["database"])
        #expect(document.services[1].variables.values == ["DEBUG": "true", "EMPTY": ""])
        #expect(document.services[2].workingDirectory == ".")
        #expect(!document.services[2].portCanChange)
        #expect(document.profiles.isEmpty)
    }

    @Test(arguments: [
        // What is wrong, the line and field it is reported at.
        ("name: x\n", LocalhostError.invalidProjectFile(.notAProjectFile, field: [], line: nil)),
        ("localhost: 2\nname: x\n", .invalidProjectFile(.newerFormat(2), field: ["localhost"], line: 1)),
        ("localhost: one\n", .invalidProjectFile(.expected(.number), field: ["localhost"], line: 1)),
        ("localhost: 1\nkey: a\npath: /a\n", .invalidProjectFile(.missing, field: ["name"], line: 1)),
        ("localhost: 1\nname: A\nkey: A B\npath: /a\n", .invalidProjectFile(.invalid(.invalidKey("A B")), field: ["key"], line: 3)),
        ("localhost: 1\nname: A\nkey: a\npath: a\n", .invalidProjectFile(.invalid(.invalidProjectPath("a")), field: ["path"], line: 4)),
        ("localhost: 1\nname: A\nkey: a\npath: /a\ncolour: red\n", .invalidProjectFile(.unknownField, field: ["colour"], line: 5)),
        ("localhost: 1\nname: A\nname: B\n", .invalidProjectFile(.duplicate, field: ["name"], line: 3)),
        ("localhost: 1\nname: |\n  A\n", .invalidProjectFile(.unsupportedYAML("block scalars (| and >)"), field: [], line: 2)),
        ("localhost: 1\nname: \"A\n", .invalidProjectFile(.unterminatedQuote, field: [], line: 2)),
        ("localhost: 1\nname: A\n   key: a\n", .invalidProjectFile(.unexpectedIndentation, field: [], line: 3)),
        ("localhost: 1\njust words\n", .invalidProjectFile(.expectedKeyValue, field: [], line: 2)),
        ("localhost: 1\nname: A\nkey: a\npath: /a\nservices:\n  web:\n    cwd: web\n",
         .invalidProjectFile(.missing, field: ["services", "web", "command"], line: 6)),
        ("localhost: 1\nname: A\nkey: a\npath: /a\nservices:\n  Web:\n    command: x\n",
         .invalidProjectFile(.invalid(.invalidKey("Web")), field: ["services", "Web"], line: 6)),
        ("localhost: 1\nname: A\nkey: a\npath: /a\nservices:\n  web:\n    command: x\n    port: 70000\n",
         .invalidProjectFile(.invalid(.invalidPort(70000)), field: ["services", "web", "port"], line: 8)),
        ("localhost: 1\nname: A\nkey: a\npath: /a\nservices:\n  web:\n    command: x\n    port: auto\n",
         .invalidProjectFile(.expected(.number), field: ["services", "web", "port"], line: 8)),
        ("localhost: 1\nname: A\nkey: a\npath: /a\nservices:\n  web:\n    command: x\n    cwd: /abs\n",
         .invalidProjectFile(.invalid(.invalidWorkingDirectory("/abs")), field: ["services", "web", "cwd"], line: 8)),
        ("localhost: 1\nname: A\nkey: a\npath: /a\nservices:\n  web:\n    command: x\n    start_all: no\n",
         .invalidProjectFile(.expected(.boolean), field: ["services", "web", "start_all"], line: 8)),
        ("localhost: 1\nname: A\nkey: a\npath: /a\nservices:\n  web:\n    command: x\n    depends_on:\n      - db\n",
         .invalidProjectFile(.unknownDependency("db"), field: ["services", "web", "depends_on"], line: 9)),
        ("localhost: 1\nname: A\nkey: a\npath: /a\nservices:\n  web:\n    command: x\n    depends_on: [api]\n  api:\n    command: y\n    depends_on: [web]\n",
         .invalidProjectFile(.invalid(.dependencyCycle([Key.test("api"), Key.test("web"), Key.test("api")])),
                             field: ["services", "api", "depends_on"], line: 11)),
        ("localhost: 1\nname: A\nkey: a\npath: /a\nvariables:\n  9LIVES: x\n",
         .invalidProjectFile(.invalid(.invalidEnvironmentVariableName("9LIVES")), field: ["variables", "9LIVES"], line: 6)),
        ("localhost: 1\nname: A\nkey: a\npath: /a\nvariables:\n  TOKEN: x\nsecrets:\n  - TOKEN\n",
         .invalidProjectFile(.invalid(.duplicateVariableName("TOKEN")), field: ["secrets"], line: 8)),
        ("localhost: 1\nname: A\nkey: a\npath: /a\nprofiles:\n  Dev: {}\nactive_profile: Prod\n",
         .invalidProjectFile(.unknownProfile("Prod"), field: ["active_profile"], line: 7)),
        ("localhost: 1\nname: A\nkey: a\npath: /a\nprofiles:\n  Dev:\n  \" Dev\": {}\n",
         .invalidProjectFile(.invalid(.duplicateProfileName("Dev")), field: ["profiles", " Dev"], line: 7)),
    ])
    func `rejects an invalid file, naming the line and field`(yaml: String, error: LocalhostError) {
        #expect(throws: error) { try ProjectDocument(yaml: yaml) }
    }

    @Test func `says where the problem is in its message`() {
        let error = LocalhostError.invalidProjectFile(.invalid(.invalidPort(70000)), field: ["services", "web", "port"], line: 8)
        #expect(error.message == "Line 8, services › web › port: 70000 is not a valid port. Use a number from 1 to 65535.")
    }

    // MARK: Importing

    @Test func `importing a Key that is taken changes nothing unless told what to do`() async throws {
        let library = try makeLibrary()
        let existing = try await makeFullProject(in: library)
        let document = try ProjectDocument(yaml: ProjectDocument(existing).yaml)

        await #expect(throws: LocalhostError.duplicateProjectKey(try Key("my-saas"))) {
            try await library.importProject(document, origin: .mainWindow)
        }
        #expect(await library.projects() == [existing])
    }

    @Test func `keep both adds the import under the next free Key`() async throws {
        let library = try makeLibrary()
        let existing = try await makeFullProject(in: library)
        try await library.addProject(name: "Taken", key: Key("my-saas-2"), path: "/tmp/taken", origin: .mainWindow)

        let imported = try await library.importProject(ProjectDocument(existing), onKeyClash: .keepBoth, origin: .mainWindow)

        #expect(imported.key.rawValue == "my-saas-3")
        #expect(imported.id != existing.id)
        #expect(try await library.project(id: existing.id) == existing)
        #expect(await library.projects().count == 3)
    }

    @Test func `replace keeps the Project's identity, matching Services and Profiles, and their Secret values`() async throws {
        let library = try makeLibrary()
        let existing = try await makeFullProject(in: library)
        let api = try #require(existing.services.first { $0.key.rawValue == "api" })
        let web = try #require(existing.services.first { $0.key.rawValue == "web" })
        try await library.editService(id: web.id, variables: [.secret("SESSION_SECRET", "s3cret")], origin: .mainWindow)

        // The file drops `web` and the Project's Secret, renames the Project and adds a Service.
        let yaml = ProjectDocument(existing).yaml
            .replacingOccurrences(of: "name: My SaaS", with: "name: My SaaS (shared)")
            .replacingOccurrences(of: "secrets:\n  - SENTRY_DSN\n", with: "")
            .replacingOccurrences(
                of: "  web:\n    name: Web\n    command: pnpm dev\n    cwd: web\n    port: 3000\n    url: http://localhost:3000/admin\n    start_all: false\n    depends_on:\n      - api\n",
                with: "  worker:\n    command: uv run worker\n"
            )
        let replaced = try await library.importProject(ProjectDocument(yaml: yaml), onKeyClash: .replace, origin: .mainWindow)

        #expect(replaced.id == existing.id)
        #expect(replaced.sortOrder == existing.sortOrder)
        #expect(replaced.isFavorite)
        #expect(replaced.name == "My SaaS (shared)")
        #expect(replaced.services.map(\.key.rawValue) == ["database", "api", "worker"])
        #expect(replaced.services[1].id == api.id)
        #expect(replaced.profiles.map(\.id) == existing.profiles.map(\.id))
        #expect(replaced.activeProfileID == existing.activeProfileID)
        #expect(try await library.secretValue(named: "STRIPE_KEY", in: .service(api.id)) == "sk_test_123")
        // Secrets the import no longer names are removed.
        #expect(secrets.accounts == ["\(api.id.uuidString)/STRIPE_KEY"])
        #expect(await library.projects() == [replaced])
        #expect(try await makeLibrary().projects() == [replaced])
    }

    @Test func `imports into the folder the review picked`() async throws {
        var document = try ProjectDocument(yaml: "localhost: 1\nname: A\nkey: a\npath: /Users/someone-else/a\n")
        document.path = "~/Elsewhere/a/"

        let imported = try await makeLibrary().importProject(document, origin: .mainWindow)

        #expect(imported.path == "\(home)/Elsewhere/a")
    }
}

extension Key {
    /// A Key known to be valid, for test data.
    static func test(_ raw: String) -> Key {
        try! Key(raw)
    }
}
