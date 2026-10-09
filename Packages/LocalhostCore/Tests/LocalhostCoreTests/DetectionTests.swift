import Foundation
import Testing
@testable import LocalhostCore

/// Scanning against fixture directories. Each test writes a small Project
/// folder, file by file, and checks the suggested Services.
final class DetectionTests {
    let root = FileManager.default.temporaryDirectory
        .appending(path: "LocalhostDetectionTests-\(UUID().uuidString)", directoryHint: .isDirectory)

    deinit {
        try? FileManager.default.removeItem(at: root)
    }

    /// Writes `files` (relative path → contents) into a fresh Project folder
    /// and scans it.
    func scan(_ files: [String: String]) async throws -> [ServiceSuggestion] {
        for (path, contents) in files {
            let url = root.appending(path: path)
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try contents.write(to: url, atomically: true, encoding: .utf8)
        }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return await ProjectScanner().scan(projectAt: root.path)
    }

    func packageJSON(name: String? = nil, scripts: [(String, String)], extra: String = "") -> String {
        let scriptLines = scripts.map { "    \"\($0.0)\": \"\($0.1)\"" }.joined(separator: ",\n")
        let nameLine = name.map { "  \"name\": \"\($0)\",\n" } ?? ""
        return "{\n\(nameLine)\(extra)  \"scripts\": {\n\(scriptLines)\n  }\n}\n"
    }

    // MARK: Scanning

    @Test func `a folder with nothing recognisable yields no suggestions`() async throws {
        let suggestions = try await scan(["README.md": "# Hello", "notes/todo.txt": "x"])
        #expect(suggestions.isEmpty)
    }

    @Test func `dependency, build and hidden folders are not scanned`() async throws {
        let dev = packageJSON(scripts: [("dev", "vite")])
        let suggestions = try await scan([
            "node_modules/some-lib/package.json": dev,
            ".cache/tool/package.json": dev,
            "dist/package.json": dev,
        ])
        #expect(suggestions.isEmpty)
    }

    @Test func `suggested keys are unique across detectors`() async throws {
        let suggestions = try await scan([
            "compose.yaml": "services:\n  web:\n    image: nginx\n",
            "package.json": packageJSON(name: "web", scripts: [("dev", "vite")]),
        ])
        #expect(suggestions.map(\.key.rawValue) == ["web", "web-2"])
    }

    // MARK: Node

    @Test func `package.json scripts use the lockfile's package manager and pass $PORT to Vite`() async throws {
        let suggestions = try await scan([
            "package.json": packageJSON(name: "shop", scripts: [
                ("build", "vite build"), ("dev", "vite"), ("postinstall", "patch-package"), ("lint", "eslint ."),
            ]),
            "pnpm-lock.yaml": "",
        ])

        let dev = try #require(suggestions.first)
        #expect(dev.name == "shop")
        #expect(dev.key.rawValue == "shop")
        #expect(dev.workingDirectory == ".")
        #expect(dev.commandLine == "pnpm run dev --port $PORT")
        #expect(dev.port == 5173)
        #expect(dev.portCanChange)
        #expect(dev.isRecommended)
        #expect(dev.foundIn == "package.json")
        #expect(dev.detail == "\"dev\": vite")

        #expect(suggestions.dropFirst().map(\.name) == ["shop build", "shop lint"])
        #expect(suggestions.dropFirst().allSatisfy { !$0.isRecommended })
        #expect(suggestions[1].commandLine == "pnpm run build")
        #expect(suggestions[1].port == nil)
    }

    @Test(arguments: [
        ("yarn.lock", "yarn run dev --port $PORT"),
        ("bun.lockb", "bun run dev --port $PORT"),
        ("bun.lock", "bun run dev --port $PORT"),
        ("package-lock.json", "npm run dev -- --port $PORT"),
        ("no-lockfile.txt", "npm run dev -- --port $PORT"),
    ])
    func `the lockfile picks the package manager`(lockfile: String, commandLine: String) async throws {
        let suggestions = try await scan([
            "package.json": packageJSON(scripts: [("dev", "vite")]),
            lockfile: "",
        ])
        #expect(suggestions.first?.commandLine == commandLine)
    }

    @Test func `the packageManager field wins over lockfiles`() async throws {
        let suggestions = try await scan([
            "package.json": packageJSON(scripts: [("dev", "vite")], extra: "  \"packageManager\": \"pnpm@9.1.0\",\n"),
            "package-lock.json": "",
        ])
        #expect(suggestions.first?.commandLine == "pnpm run dev --port $PORT")
    }

    @Test func `Next reads PORT itself, so its script runs as written`() async throws {
        let suggestions = try await scan([
            "package.json": packageJSON(scripts: [("dev", "next dev"), ("start", "next start")]),
            "package-lock.json": "",
        ])
        #expect(suggestions.map(\.name) == ["dev", "start"])
        #expect(suggestions[0].commandLine == "npm run dev")
        #expect(suggestions[0].port == 3000)
        #expect(suggestions[0].isRecommended)
        #expect(!suggestions[1].isRecommended, "start is usually production when dev exists")
    }

    @Test func `start is checked when there is no dev script`() async throws {
        let suggestions = try await scan([
            "package.json": packageJSON(scripts: [("test", "react-scripts test"), ("start", "react-scripts start")]),
        ])
        #expect(suggestions.first?.name == "start")
        #expect(suggestions.first?.isRecommended == true)
        #expect(suggestions.first?.port == 3000)
        #expect(suggestions.filter(\.isRecommended).count == 1)
    }

    @Test func `a port written into the script is kept and can't move`() async throws {
        let suggestions = try await scan([
            "package.json": packageJSON(scripts: [("dev", "vite --port 3001"), ("storybook", "storybook dev -p 6006")]),
        ])
        #expect(suggestions[0].commandLine == "npm run dev")
        #expect(suggestions[0].port == 3001)
        #expect(!suggestions[0].portCanChange)
        #expect(suggestions[1].port == 6006)
        #expect(!suggestions[1].portCanChange)
    }

    @Test func `a script that already passes $PORT gets the default port without another flag`() async throws {
        let suggestions = try await scan([
            "package.json": packageJSON(scripts: [("dev", "astro dev --port $PORT")]),
        ])
        #expect(suggestions.first?.commandLine == "npm run dev")
        #expect(suggestions.first?.port == 4321)
        #expect(suggestions.first?.portCanChange == true)
    }

    @Test(arguments: ["concurrently \\\"vite\\\" \\\"tsc -w\\\"", "tsc && vite", "node server.js", "vite build"])
    func `unknown or compound scripts get no port`(script: String) async throws {
        let suggestions = try await scan(["package.json": packageJSON(scripts: [("dev", script)])])
        #expect(suggestions.first?.port == nil)
        #expect(suggestions.first?.commandLine == "npm run dev")
    }

    @Test func `wrappers and variables before the tool are skipped`() async throws {
        let suggestions = try await scan([
            "package.json": packageJSON(scripts: [("dev", "cross-env NODE_ENV=development vite")]),
        ])
        #expect(suggestions.first?.port == 5173)
    }

    @Test func `script names that need quoting are quoted`() async throws {
        let suggestions = try await scan([
            "package.json": packageJSON(scripts: [("dev:web", "vite"), ("do it", "echo")]),
        ])
        #expect(suggestions.map(\.commandLine) == ["npm run dev:web -- --port $PORT", "npm run 'do it'"])
        #expect(suggestions[0].isRecommended)
    }

    // MARK: Workspaces

    @Test func `pnpm workspace packages are suggested with their folders and the root's scripts stay unchecked`() async throws {
        let suggestions = try await scan([
            "package.json": packageJSON(name: "acme", scripts: [("dev", "turbo dev"), ("build", "turbo build")]),
            "pnpm-workspace.yaml": "packages:\n  - \"apps/*\"\n  - 'packages/**'\n  - '!packages/**/fixtures'\n",
            "pnpm-lock.yaml": "",
            "apps/web/package.json": packageJSON(name: "@acme/web", scripts: [("dev", "next dev")]),
            "apps/docs/package.json": packageJSON(name: "@acme/docs", scripts: [("dev", "astro dev")]),
            "packages/ui/package.json": packageJSON(name: "@acme/ui", scripts: [("build", "tsup")]),
            "packages/ui/fixtures/package.json": packageJSON(scripts: [("dev", "vite")]),
            "examples/demo/package.json": packageJSON(name: "demo", scripts: [("dev", "vite")]),
        ])

        let checked = suggestions.filter(\.isRecommended)
        #expect(checked.map(\.key.rawValue) == ["docs", "web"])
        #expect(checked.map(\.workingDirectory) == ["apps/docs", "apps/web"])
        #expect(checked.map(\.commandLine) == ["pnpm run dev --port $PORT", "pnpm run dev"])
        #expect(checked.map(\.foundIn) == ["apps/docs/package.json", "apps/web/package.json"])

        #expect(suggestions.contains { $0.name == "acme" && $0.workingDirectory == "." && !$0.isRecommended })
        #expect(suggestions.contains { $0.name == "ui build" && $0.workingDirectory == "packages/ui" })
        #expect(suggestions.contains { $0.workingDirectory == "examples/demo" && !$0.isRecommended },
                "folders outside the workspaces are listed, unchecked")
        #expect(!suggestions.contains { $0.workingDirectory == "packages/ui/fixtures" })
    }

    @Test func `npm and Yarn workspaces come from package.json, deeper than the scan depth`() async throws {
        let suggestions = try await scan([
            "package.json": packageJSON(scripts: [("dev", "lerna run dev")],
                                        extra: "  \"workspaces\": { \"packages\": [\"services/*/app\"] },\n"),
            "yarn.lock": "",
            "services/billing/app/package.json": packageJSON(name: "billing", scripts: [("start", "node index.js")]),
        ])
        let billing = try #require(suggestions.filter(\.isRecommended).first)
        #expect(billing.workingDirectory == "services/billing/app")
        #expect(billing.commandLine == "yarn run start")
    }

    // MARK: Compose

    @Test func `each Compose service runs in the foreground with its published port`() async throws {
        let suggestions = try await scan([
            "compose.yaml": """
            # local infrastructure
            name: acme
            services:
              postgres:
                image: postgres:16 # pinned
                ports:
                  - "5432:5432"
                environment:
                  POSTGRES_PASSWORD: postgres
              redis:
                image: "redis:7"
                ports: ["127.0.0.1:6379:6379/tcp"]
              mail:
                image: axllent/mailpit
                ports:
                - target: 8025
                  published: "8025"
                - 1025:1025
              api:
                build: ./api
                ports:
                  - "${API_PORT:-8000}:8000"
              worker:
                build: ./worker
                ports:
                  - "9000"
            volumes:
              data: {}
            """,
        ])

        #expect(suggestions.map(\.key.rawValue) == ["postgres", "redis", "mail", "api", "worker"])
        #expect(suggestions.map(\.port) == [5432, 6379, 8025, 8000, nil])
        #expect(suggestions[0].commandLine == "docker compose up postgres")
        #expect(suggestions[0].workingDirectory == ".")
        #expect(suggestions[0].detail == "Compose service · postgres:16")
        #expect(suggestions.allSatisfy { $0.isRecommended && !$0.portCanChange && $0.foundIn == "compose.yaml" })
    }

    @Test func `only the Compose file Docker would pick is read`() async throws {
        let suggestions = try await scan([
            "infra/docker-compose.yml": "services:\n  old:\n    image: a\n",
            "infra/compose.yml": "services:\n  db:\n    image: postgres\n",
        ])
        #expect(suggestions.map(\.name) == ["db"])
        #expect(suggestions.first?.workingDirectory == "infra")
        #expect(suggestions.first?.foundIn == "infra/compose.yml")
    }

    // MARK: Python

    @Test func `a uv FastAPI app runs under uvicorn with $PORT`() async throws {
        let suggestions = try await scan([
            "pyproject.toml": """
            [project]
            name = "api"
            dependencies = [
              "fastapi",   # web
              "uvicorn",
            ]

            [project.scripts]
            seed = "api.seed:main"
            """,
            "uv.lock": "",
            "app/__init__.py": "",
            "app/main.py": "from fastapi import FastAPI\n\napp = FastAPI(title=\"API\")\n",
            "tests/test_main.py": "app = FastAPI()\n",
        ])

        let api = try #require(suggestions.first)
        #expect(api.name == "api")
        #expect(api.commandLine == "uv run uvicorn app.main:app --reload --port $PORT")
        #expect(api.port == 8000)
        #expect(api.isRecommended)
        #expect(api.foundIn == "app/main.py")
        #expect(suggestions.last?.commandLine == "uv run seed")
        #expect(suggestions.last?.isRecommended == false)
    }

    @Test func `a FastAPI app in a src layout gets --app-dir`() async throws {
        let suggestions = try await scan([
            "pyproject.toml": "[project]\nname = \"svc\"\n[tool.uv]\ndev-dependencies = []\n",
            "src/svc/api.py": "import fastapi\napi: fastapi.FastAPI = fastapi.FastAPI()\n",
        ])
        #expect(suggestions.first?.commandLine == "uv run uvicorn svc.api:api --reload --app-dir src --port $PORT")
    }

    @Test func `a Poetry Django project runs manage.py runserver`() async throws {
        let suggestions = try await scan([
            "backend/pyproject.toml": "[tool.poetry]\nname = \"shop-backend\"\n",
            "backend/manage.py": "import django\n",
        ])
        let django = try #require(suggestions.first)
        #expect(django.workingDirectory == "backend")
        #expect(django.commandLine == "poetry run python manage.py runserver $PORT")
        #expect(django.port == 8000)
        #expect(django.detail == "Django")
    }

    @Test func `a Flask app with a virtual environment uses its tools`() async throws {
        let suggestions = try await scan([
            "requirements.txt": "flask\n",
            ".venv/bin/python": "",
            "app.py": "from flask import Flask\napp = Flask(__name__)\n",
        ])
        #expect(suggestions.first?.commandLine == ".venv/bin/flask --app app:app run --debug --port $PORT")
        #expect(suggestions.first?.port == 5000)
    }

    @Test func `without a tool or environment the system python3 runs it`() async throws {
        let suggestions = try await scan([
            "requirements.txt": "fastapi\nuvicorn\n",
            "main.py": "from fastapi import FastAPI\nserver = FastAPI()\n",
        ])
        #expect(suggestions.first?.commandLine == "python3 -m uvicorn main:server --reload --port $PORT")
    }

    @Test func `python files without a project file are ignored`() async throws {
        let suggestions = try await scan(["scripts/main.py": "app = FastAPI()\n"])
        #expect(suggestions.isEmpty)
    }

    // MARK: Cargo

    @Test func `a Cargo binary package runs with cargo run, extra binaries unchecked`() async throws {
        let suggestions = try await scan([
            "Cargo.toml": "[package]\nname = \"server\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"migrate\"\npath = \"src/migrate.rs\"\n",
            "src/main.rs": "fn main() {}",
            "src/bin/seed.rs": "fn main() {}",
        ])
        #expect(suggestions.map(\.commandLine) == ["cargo run", "cargo run --bin migrate", "cargo run --bin seed"])
        #expect(suggestions.map(\.isRecommended) == [true, false, false])
        #expect(suggestions.first?.key.rawValue == "server")
    }

    @Test func `a Cargo library or virtual workspace root gets nothing, its members do`() async throws {
        let suggestions = try await scan([
            "Cargo.toml": "[workspace]\nmembers = [\n  \"crates/*\",\n]\n",
            "crates/core/Cargo.toml": "[package]\nname = \"core\"\n",
            "crates/core/src/lib.rs": "",
            "crates/cli/Cargo.toml": "[package]\nname = \"acme-cli\"\n",
            "crates/cli/src/main.rs": "fn main() {}",
        ])
        #expect(suggestions.map(\.name) == ["acme-cli"])
        #expect(suggestions.first?.workingDirectory == "crates/cli")
    }

    // MARK: Acceptance

    /// The M2 acceptance Project: a pnpm monorepo with a Vite web app, a
    /// FastAPI API managed by uv and Postgres in Compose.
    @Test func `a pnpm + FastAPI + Compose monorepo yields ready-to-start suggestions`() async throws {
        let suggestions = try await scan([
            "package.json": packageJSON(name: "acme", scripts: [("dev", "turbo dev")],
                                        extra: "  \"packageManager\": \"pnpm@9.12.0\",\n"),
            "pnpm-workspace.yaml": "packages:\n  - apps/*\n",
            "pnpm-lock.yaml": "",
            "compose.yaml": "services:\n  postgres:\n    image: postgres:16\n    ports:\n      - 5432:5432\n",
            "apps/web/package.json": packageJSON(name: "@acme/web", scripts: [("dev", "vite"), ("build", "vite build")]),
            "apps/api/pyproject.toml": "[project]\nname = \"api\"\nrequires-python = \">=3.12\"\n",
            "apps/api/uv.lock": "",
            "apps/api/app/main.py": "from fastapi import FastAPI\napp = FastAPI()\n",
        ])

        let checked = suggestions.filter(\.isRecommended).map {
            [$0.key.rawValue, $0.workingDirectory, $0.commandLine, $0.port.map(String.init) ?? "-"]
        }
        #expect(checked == [
            ["postgres", ".", "docker compose up postgres", "5432"],
            ["api", "apps/api", "uv run uvicorn app.main:app --reload --port $PORT", "8000"],
            ["web", "apps/web", "pnpm run dev --port $PORT", "5173"],
        ])
    }

    // MARK: Readers

    @Test(arguments: [
        ("apps/*", "apps/web", true), ("apps/*", "apps/web/src", false), ("packages/**", "packages/a/b", true),
        ("**/fixtures", "packages/x/fixtures", true), ("app-*", "app-web", true), ("app-*", "lib-web", false),
    ])
    func `workspace globs match folders`(pattern: String, path: String, matches: Bool) {
        #expect(ScannedDirectory.glob(pattern, matches: path) == matches)
    }

    @Test func `the TOML reader keeps tables, array tables and multi-line arrays`() {
        let document = TOMLDocument("""
        name = "top"
        [package]
        name = "x" # comment
        [[bin]]
        name = 'a'
        [[bin]]
        name = "b"
        [workspace]
        members = [
          "one", # first
          "two/*",
        ]
        """)
        #expect(TOMLDocument.string(document.table("")?.values["name"]) == "top")
        #expect(TOMLDocument.string(document.table("package")?.values["name"]) == "x")
        #expect(document.tables(named: "bin").compactMap { TOMLDocument.string($0.values["name"]) } == ["a", "b"])
        #expect(TOMLDocument.strings(document.table("workspace")?.values["members"]) == ["one", "two/*"])
    }
}
