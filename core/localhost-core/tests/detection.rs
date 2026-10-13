//! Ported from `DetectionTests.swift`: scanning against fixture folders.
//! Each test writes a small Project folder, file by file, and checks the
//! suggested Services.

mod common;

use common::{Fixture, key};
use localhost_core::{NewService, Origin, ProjectScanner, ServiceSuggestion};

/// Writes `files` (relative path → contents) into a fresh Project folder and scans it.
fn scan(files: &[(&str, &str)]) -> (tempfile::TempDir, Vec<ServiceSuggestion>) {
    let root = tempfile::tempdir().unwrap();
    for (path, contents) in files {
        let path = root.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    let suggestions = ProjectScanner::default().scan_folder(root.path().to_str().unwrap());
    (root, suggestions)
}

fn suggestions(files: &[(&str, &str)]) -> Vec<ServiceSuggestion> {
    scan(files).1
}

fn package_json(name: Option<&str>, scripts: &[(&str, &str)], extra: &str) -> String {
    let script_lines: Vec<String> = scripts.iter().map(|(name, body)| format!("    \"{name}\": \"{body}\"")).collect();
    let name_line = name.map(|name| format!("  \"name\": \"{name}\",\n")).unwrap_or_default();
    format!("{{\n{name_line}{extra}  \"scripts\": {{\n{}\n  }}\n}}\n", script_lines.join(",\n"))
}

fn scripts(scripts: &[(&str, &str)]) -> String {
    package_json(None, scripts, "")
}

fn field<T>(suggestions: &[ServiceSuggestion], get: impl Fn(&ServiceSuggestion) -> T) -> Vec<T> {
    suggestions.iter().map(get).collect()
}

// MARK: Scanning

#[test]
fn a_folder_with_nothing_recognisable_yields_no_suggestions() {
    assert!(suggestions(&[("README.md", "# Hello"), ("notes/todo.txt", "x")]).is_empty());
}

#[test]
fn dependency_build_and_hidden_folders_are_not_scanned() {
    let dev = scripts(&[("dev", "vite")]);
    assert!(
        suggestions(&[
            ("node_modules/some-lib/package.json", &dev),
            (".cache/tool/package.json", &dev),
            ("dist/package.json", &dev)
        ])
        .is_empty()
    );
}

#[test]
fn suggested_keys_are_unique_across_detectors() {
    let found = suggestions(&[
        ("compose.yaml", "services:\n  web:\n    image: nginx\n"),
        ("package.json", &package_json(Some("web"), &[("dev", "vite")], "")),
    ]);
    assert_eq!(field(&found, |s| s.key.to_string()), ["web", "web-2"]);
}

#[test]
fn rescanning_a_project_hides_what_its_services_cover_and_avoids_their_keys() {
    let (root, _) = scan(&[
        ("package.json", &package_json(Some("web"), &[("dev", "vite"), ("build", "vite build")], "")),
        ("compose.yaml", "services:\n  postgres:\n    image: postgres\n"),
    ]);
    let fixture = Fixture::new();
    let project = fixture
        .library()
        .add_project(
            "Web",
            key("web"),
            root.path().to_str().unwrap(),
            vec![
                NewService::new("Postgres", key("postgres"), "docker compose up postgres"),
                NewService::new("Web", key("web"), "npm start"),
            ],
            Origin::MainWindow,
        )
        .unwrap();

    let found = ProjectScanner::default().scan_project(&project);
    assert_eq!(field(&found, |s| s.command_line.clone()), ["npm run dev -- --port $PORT", "npm run build"]);
    assert_eq!(field(&found, |s| s.key.to_string()), ["web-2", "web-build"]);
}

// MARK: Node

#[test]
fn package_json_scripts_use_the_lockfiles_package_manager_and_pass_port_to_vite() {
    let found = suggestions(&[
        (
            "package.json",
            &package_json(
                Some("shop"),
                &[("build", "vite build"), ("dev", "vite"), ("postinstall", "patch-package"), ("lint", "eslint .")],
                "",
            ),
        ),
        ("pnpm-lock.yaml", ""),
    ]);

    let dev = &found[0];
    assert_eq!(dev.name, "shop");
    assert_eq!(dev.key.as_str(), "shop");
    assert_eq!(dev.working_directory, ".");
    assert_eq!(dev.command_line, "pnpm run dev --port $PORT");
    assert_eq!(dev.port, Some(5173));
    assert!(dev.port_can_change);
    assert!(dev.is_recommended);
    assert_eq!(dev.found_in, "package.json");
    assert_eq!(dev.detail, "\"dev\": vite");

    assert_eq!(field(&found[1..], |s| s.name.clone()), ["shop build", "shop lint"]);
    assert!(found[1..].iter().all(|s| !s.is_recommended));
    assert_eq!(found[1].command_line, "pnpm run build");
    assert_eq!(found[1].port, None);
}

#[test]
fn the_lockfile_picks_the_package_manager() {
    for (lockfile, command_line) in [
        ("yarn.lock", "yarn run dev --port $PORT"),
        ("bun.lockb", "bun run dev --port $PORT"),
        ("bun.lock", "bun run dev --port $PORT"),
        ("package-lock.json", "npm run dev -- --port $PORT"),
        ("no-lockfile.txt", "npm run dev -- --port $PORT"),
    ] {
        let found = suggestions(&[("package.json", &scripts(&[("dev", "vite")])), (lockfile, "")]);
        assert_eq!(found[0].command_line, command_line, "{lockfile}");
    }
}

#[test]
fn the_package_manager_field_wins_over_lockfiles() {
    let found = suggestions(&[
        ("package.json", &package_json(None, &[("dev", "vite")], "  \"packageManager\": \"pnpm@9.1.0\",\n")),
        ("package-lock.json", ""),
    ]);
    assert_eq!(found[0].command_line, "pnpm run dev --port $PORT");
}

#[test]
fn next_reads_port_itself_so_its_script_runs_as_written() {
    let found = suggestions(&[
        ("package.json", &scripts(&[("dev", "next dev"), ("start", "next start")])),
        ("package-lock.json", ""),
    ]);
    assert_eq!(field(&found, |s| s.name.clone()), ["dev", "start"]);
    assert_eq!(found[0].command_line, "npm run dev");
    assert_eq!(found[0].port, Some(3000));
    assert!(found[0].is_recommended);
    assert!(!found[1].is_recommended, "start is usually production when dev exists");
}

#[test]
fn start_is_checked_when_there_is_no_dev_script() {
    let found =
        suggestions(&[("package.json", &scripts(&[("test", "react-scripts test"), ("start", "react-scripts start")]))]);
    assert_eq!(found[0].name, "start");
    assert!(found[0].is_recommended);
    assert_eq!(found[0].port, Some(3000));
    assert_eq!(found.iter().filter(|s| s.is_recommended).count(), 1);
}

#[test]
fn a_port_written_into_the_script_is_kept_and_cant_move() {
    let found = suggestions(&[(
        "package.json",
        &scripts(&[("dev", "vite --port 3001"), ("storybook", "storybook dev -p 6006")]),
    )]);
    assert_eq!(found[0].command_line, "npm run dev");
    assert_eq!(found[0].port, Some(3001));
    assert!(!found[0].port_can_change);
    assert_eq!(found[1].port, Some(6006));
    assert!(!found[1].port_can_change);
}

#[test]
fn a_script_that_already_passes_port_gets_the_default_port_without_another_flag() {
    let found = suggestions(&[("package.json", &scripts(&[("dev", "astro dev --port $PORT")]))]);
    assert_eq!(found[0].command_line, "npm run dev");
    assert_eq!(found[0].port, Some(4321));
    assert!(found[0].port_can_change);
}

#[test]
fn unknown_or_compound_scripts_get_no_port() {
    for script in [r#"concurrently \"vite\" \"tsc -w\""#, "tsc && vite", "node server.js", "vite build"] {
        let found = suggestions(&[("package.json", &scripts(&[("dev", script)]))]);
        assert_eq!(found[0].port, None, "{script}");
        assert_eq!(found[0].command_line, "npm run dev");
    }
}

#[test]
fn wrappers_and_variables_before_the_tool_are_skipped() {
    let found = suggestions(&[("package.json", &scripts(&[("dev", "cross-env NODE_ENV=development vite")]))]);
    assert_eq!(found[0].port, Some(5173));
}

#[test]
fn script_names_that_need_quoting_are_quoted() {
    let found = suggestions(&[("package.json", &scripts(&[("dev:web", "vite"), ("do it", "echo")]))]);
    assert_eq!(field(&found, |s| s.command_line.clone()), ["npm run dev:web -- --port $PORT", "npm run 'do it'"]);
    assert!(found[0].is_recommended);
}

// MARK: Workspaces

#[test]
fn pnpm_workspace_packages_are_suggested_with_their_folders_and_the_roots_scripts_stay_unchecked() {
    let found = suggestions(&[
        ("package.json", &package_json(Some("acme"), &[("dev", "turbo dev"), ("build", "turbo build")], "")),
        ("pnpm-workspace.yaml", "packages:\n  - \"apps/*\"\n  - 'packages/**'\n  - '!packages/**/fixtures'\n"),
        ("pnpm-lock.yaml", ""),
        ("apps/web/package.json", &package_json(Some("@acme/web"), &[("dev", "next dev")], "")),
        ("apps/docs/package.json", &package_json(Some("@acme/docs"), &[("dev", "astro dev")], "")),
        ("packages/ui/package.json", &package_json(Some("@acme/ui"), &[("build", "tsup")], "")),
        ("packages/ui/fixtures/package.json", &scripts(&[("dev", "vite")])),
        ("examples/demo/package.json", &package_json(Some("demo"), &[("dev", "vite")], "")),
    ]);

    let checked: Vec<&ServiceSuggestion> = found.iter().filter(|s| s.is_recommended).collect();
    assert_eq!(checked.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["docs", "web"]);
    assert_eq!(checked.iter().map(|s| s.working_directory.as_str()).collect::<Vec<_>>(), ["apps/docs", "apps/web"]);
    assert_eq!(
        checked.iter().map(|s| s.command_line.as_str()).collect::<Vec<_>>(),
        ["pnpm run dev --port $PORT", "pnpm run dev"]
    );
    assert_eq!(
        checked.iter().map(|s| s.found_in.as_str()).collect::<Vec<_>>(),
        ["apps/docs/package.json", "apps/web/package.json"]
    );

    assert!(found.iter().any(|s| s.name == "acme" && s.working_directory == "." && !s.is_recommended));
    assert!(found.iter().any(|s| s.name == "ui build" && s.working_directory == "packages/ui"));
    assert!(
        found.iter().any(|s| s.working_directory == "examples/demo" && !s.is_recommended),
        "folders outside the workspaces are listed, unchecked"
    );
    assert!(!found.iter().any(|s| s.working_directory == "packages/ui/fixtures"));
}

#[test]
fn npm_and_yarn_workspaces_come_from_package_json_deeper_than_the_scan_depth() {
    let found = suggestions(&[
        (
            "package.json",
            &package_json(
                None,
                &[("dev", "lerna run dev")],
                "  \"workspaces\": { \"packages\": [\"services/*/app\"] },\n",
            ),
        ),
        ("yarn.lock", ""),
        ("services/billing/app/package.json", &package_json(Some("billing"), &[("start", "node index.js")], "")),
    ]);
    let billing = found.iter().find(|s| s.is_recommended).unwrap();
    assert_eq!(billing.working_directory, "services/billing/app");
    assert_eq!(billing.command_line, "yarn run start");
}

// MARK: Compose

#[test]
fn each_compose_service_runs_in_the_foreground_with_its_published_port() {
    let found = suggestions(&[(
        "compose.yaml",
        r#"# local infrastructure
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
  data: {}"#,
    )]);

    assert_eq!(field(&found, |s| s.key.to_string()), ["postgres", "redis", "mail", "api", "worker"]);
    assert_eq!(field(&found, |s| s.port), [Some(5432), Some(6379), Some(8025), Some(8000), None]);
    assert_eq!(found[0].command_line, "docker compose up postgres");
    assert_eq!(found[0].working_directory, ".");
    assert_eq!(found[0].detail, "Compose service · postgres:16");
    assert!(found.iter().all(|s| s.is_recommended && !s.port_can_change && s.found_in == "compose.yaml"));
}

#[test]
fn only_the_compose_file_docker_would_pick_is_read() {
    let found = suggestions(&[
        ("infra/docker-compose.yml", "services:\n  old:\n    image: a\n"),
        ("infra/compose.yml", "services:\n  db:\n    image: postgres\n"),
    ]);
    assert_eq!(field(&found, |s| s.name.clone()), ["db"]);
    assert_eq!(found[0].working_directory, "infra");
    assert_eq!(found[0].found_in, "infra/compose.yml");
}

// MARK: Python

#[test]
fn a_uv_fastapi_app_runs_under_uvicorn_with_port() {
    let found = suggestions(&[
        (
            "pyproject.toml",
            "[project]\nname = \"api\"\ndependencies = [\n  \"fastapi\",   # web\n  \"uvicorn\",\n]\n\n[project.scripts]\nseed = \"api.seed:main\"\n",
        ),
        ("uv.lock", ""),
        ("app/__init__.py", ""),
        ("app/main.py", "from fastapi import FastAPI\n\napp = FastAPI(title=\"API\")\n"),
        ("tests/test_main.py", "app = FastAPI()\n"),
    ]);

    let api = &found[0];
    assert_eq!(api.name, "api");
    assert_eq!(api.command_line, "uv run uvicorn app.main:app --reload --port $PORT");
    assert_eq!(api.port, Some(8000));
    assert!(api.is_recommended);
    assert_eq!(api.found_in, "app/main.py");
    let last = found.last().unwrap();
    assert_eq!(last.command_line, "uv run seed");
    assert!(!last.is_recommended);
}

#[test]
fn a_fastapi_app_in_a_src_layout_gets_app_dir() {
    let found = suggestions(&[
        ("pyproject.toml", "[project]\nname = \"svc\"\n[tool.uv]\ndev-dependencies = []\n"),
        ("src/svc/api.py", "import fastapi\napi: fastapi.FastAPI = fastapi.FastAPI()\n"),
    ]);
    assert_eq!(found[0].command_line, "uv run uvicorn svc.api:api --reload --app-dir src --port $PORT");
}

#[test]
fn a_poetry_django_project_runs_manage_py_runserver() {
    let found = suggestions(&[
        ("backend/pyproject.toml", "[tool.poetry]\nname = \"shop-backend\"\n"),
        ("backend/manage.py", "import django\n"),
    ]);
    let django = &found[0];
    assert_eq!(django.working_directory, "backend");
    assert_eq!(django.command_line, "poetry run python manage.py runserver $PORT");
    assert_eq!(django.port, Some(8000));
    assert_eq!(django.detail, "Django");
}

#[test]
fn a_flask_app_with_a_virtual_environment_uses_its_tools() {
    let found = suggestions(&[
        ("requirements.txt", "flask\n"),
        (".venv/bin/python", ""),
        ("app.py", "from flask import Flask\napp = Flask(__name__)\n"),
    ]);
    assert_eq!(found[0].command_line, ".venv/bin/flask --app app:app run --debug --port $PORT");
    assert_eq!(found[0].port, Some(5000));
}

#[test]
fn without_a_tool_or_environment_the_system_python3_runs_it() {
    let found = suggestions(&[
        ("requirements.txt", "fastapi\nuvicorn\n"),
        ("main.py", "from fastapi import FastAPI\nserver = FastAPI()\n"),
    ]);
    assert_eq!(found[0].command_line, "python3 -m uvicorn main:server --reload --port $PORT");
}

#[test]
fn python_files_without_a_project_file_are_ignored() {
    assert!(suggestions(&[("scripts/main.py", "app = FastAPI()\n")]).is_empty());
}

// MARK: Cargo

#[test]
fn a_cargo_binary_package_runs_with_cargo_run_extra_binaries_unchecked() {
    let found = suggestions(&[
        (
            "Cargo.toml",
            "[package]\nname = \"server\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"migrate\"\npath = \"src/migrate.rs\"\n",
        ),
        ("src/main.rs", "fn main() {}"),
        ("src/bin/seed.rs", "fn main() {}"),
    ]);
    assert_eq!(
        field(&found, |s| s.command_line.clone()),
        ["cargo run", "cargo run --bin migrate", "cargo run --bin seed"]
    );
    assert_eq!(field(&found, |s| s.is_recommended), [true, false, false]);
    assert_eq!(found[0].key.as_str(), "server");
}

#[test]
fn a_cargo_library_or_virtual_workspace_root_gets_nothing_its_members_do() {
    let found = suggestions(&[
        ("Cargo.toml", "[workspace]\nmembers = [\n  \"crates/*\",\n]\n"),
        ("crates/core/Cargo.toml", "[package]\nname = \"core\"\n"),
        ("crates/core/src/lib.rs", ""),
        ("crates/cli/Cargo.toml", "[package]\nname = \"acme-cli\"\n"),
        ("crates/cli/src/main.rs", "fn main() {}"),
    ]);
    assert_eq!(field(&found, |s| s.name.clone()), ["acme-cli"]);
    assert_eq!(found[0].working_directory, "crates/cli");
}

// MARK: Acceptance

/// The M2 acceptance Project: a pnpm monorepo with a Vite web app, a FastAPI
/// API managed by uv and Postgres in Compose.
#[test]
fn a_pnpm_fastapi_compose_monorepo_yields_ready_to_start_suggestions() {
    let found = suggestions(&[
        (
            "package.json",
            &package_json(Some("acme"), &[("dev", "turbo dev")], "  \"packageManager\": \"pnpm@9.12.0\",\n"),
        ),
        ("pnpm-workspace.yaml", "packages:\n  - apps/*\n"),
        ("pnpm-lock.yaml", ""),
        ("compose.yaml", "services:\n  postgres:\n    image: postgres:16\n    ports:\n      - 5432:5432\n"),
        ("apps/web/package.json", &package_json(Some("@acme/web"), &[("dev", "vite"), ("build", "vite build")], "")),
        ("apps/api/pyproject.toml", "[project]\nname = \"api\"\nrequires-python = \">=3.12\"\n"),
        ("apps/api/uv.lock", ""),
        ("apps/api/app/main.py", "from fastapi import FastAPI\napp = FastAPI()\n"),
    ]);

    let checked: Vec<[String; 4]> = found
        .iter()
        .filter(|s| s.is_recommended)
        .map(|s| {
            [
                s.key.to_string(),
                s.working_directory.clone(),
                s.command_line.clone(),
                s.port.map_or("-".into(), |p| p.to_string()),
            ]
        })
        .collect();
    let expected = [
        ["postgres", ".", "docker compose up postgres", "5432"],
        ["api", "apps/api", "uv run uvicorn app.main:app --reload --port $PORT", "8000"],
        ["web", "apps/web", "pnpm run dev --port $PORT", "5173"],
    ];
    assert_eq!(checked, expected.map(|row| row.map(String::from)));
}

#[test]
fn a_suggestion_becomes_a_new_service_with_its_port() {
    let found = suggestions(&[("compose.yaml", "services:\n  db:\n    image: postgres\n    ports: [\"5432:5432\"]\n")]);
    let new = NewService::from(found[0].clone());
    assert_eq!((new.key.as_str(), new.port, new.port_can_change), ("db", Some(5432), false));
}
