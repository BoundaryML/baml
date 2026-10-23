//! Real CLI roots and packed home roots must not accidentally follow cwd or
//! serialized build-machine source paths. Keep all output inside temp fixtures.
mod common;

use std::{io::Write as _, path::Path, process::Command};

const RECORDINGS: &str = ".baml/btel/recordings";

fn cli(cwd: &Path, home: &Path) -> Command {
    let mut command = Command::new(common::baml_cli());
    command
        .current_dir(cwd)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("BAML_HOME", home.join(".baml"))
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .env_remove("BOUNDARY_API_KEY")
        .env("BAML_TELEMETRY", "medium");
    command
}

fn run(command: &mut Command, success: bool) {
    let output = command.output().unwrap();
    assert_eq!(
        output.status.success(),
        success,
        "{command:?}:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("telemetry recording failed"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_recordings(root: &Path, count: usize) {
    let directories: Vec<_> = std::fs::read_dir(root.join(RECORDINGS))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(directories.len(), count);
    for directory in directories {
        let recording = btel_file::read_directory(&directory).unwrap();
        assert!(!recording.files.is_empty(), "{}", directory.display());
        assert!(recording.issues.is_empty(), "{:?}", recording.issues);
        assert!(recording.files.iter().any(|file| {
            file.aggregates
                .as_ref()
                .is_some_and(|batch| !batch.entries.is_empty())
        }));
    }
}

fn home(root: &Path) -> std::path::PathBuf {
    let home = root.join("home");
    std::fs::create_dir_all(home.join(".baml")).unwrap();
    std::fs::write(
        home.join(".baml/config.toml"),
        "[update]\nauto_check = false\n",
    )
    .unwrap();
    home
}

#[test]
fn cli_uses_project_root_for_cold_cached_nested_expression_and_test_execution() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    std::fs::create_dir(&project).unwrap();
    common::write_project(
        &project,
        r#"
function main() -> int { 7 }
function fail() -> int { assert.is_true(false); 0 }
testset "smoke" { test "ok" { assert.is_true(true) } }
"#,
    );
    let home = home(temp.path());
    let nested = project.join("baml_src/nested");
    std::fs::create_dir(&nested).unwrap();
    // --from must win over both cwd and the baml_src subdirectory. Repeat to
    // cover a bytecode-cache hit as well as compilation.
    for count in 1..=2 {
        run(
            cli(temp.path(), &home)
                .args(["run", "main", "--from"])
                .arg(&project),
            true,
        );
        assert_recordings(&project, count);
    }
    run(cli(&nested, &home).args(["run", "main"]), true);
    assert_recordings(&project, 3);
    run(
        cli(temp.path(), &home)
            .args(["run", "-e", "1 + 2", "--from"])
            .arg(&project),
        true,
    );
    assert_recordings(&project, 4);
    run(
        cli(temp.path(), &home)
            .args(["test", "--from"])
            .arg(&project),
        true,
    );
    assert_recordings(&project, 5);
    run(
        cli(temp.path(), &home)
            .args(["run", "fail", "--from"])
            .arg(&project),
        false,
    );
    assert_recordings(&project, 6);
    run(
        cli(temp.path(), &home)
            .args(["run", "main", "--from"])
            .arg(&project)
            .env("BAML_TELEMETRY", "off"),
        true,
    );
    assert_recordings(&project, 6);
    assert!(!temp.path().join(RECORDINGS).exists());
    assert!(!nested.join(RECORDINGS).exists());
    assert!(!home.join(RECORDINGS).exists());
}

#[test]
fn packed_modes_write_to_user_home_even_when_launched_in_another_project() {
    let _built = common::ensure_built();
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let launch = temp.path().join("launch");
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(&launch).unwrap();
    common::write_project(&source, "function main() -> int { 9 }\n");
    common::write_project(&launch, "function main() -> int { 42 }\n");
    // Packed artifacts only accept telemetry-mode overrides explicitly allowed
    // by the publisher. This fixture tests recording roots with that opt-in.
    std::fs::OpenOptions::new()
        .append(true)
        .open(source.join("baml.toml"))
        .unwrap()
        .write_all(
            br#"
[pack.env_var_names]
BAML_TELEMETRY = "BAML_TELEMETRY"
"#,
        )
        .unwrap();
    let home = home(temp.path());
    // This case checks recording roots, not cold-cache behavior. Reuse the
    // stdlib compiled by other CLI integration tests.
    common::share_build_cache(&home.join(".baml"));
    for (index, target) in [vec!["main"], vec!["-f", "main"]].iter().enumerate() {
        let binary = temp.path().join(format!("packed-{index}"));
        run(
            cli(temp.path(), &home)
                .args(["pack", "--from"])
                .arg(&source)
                .arg("-o")
                .arg(&binary)
                .args(target),
            true,
        );
        let mut command = Command::new(&binary);
        command
            .current_dir(&launch)
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env_remove("BOUNDARY_API_KEY")
            .env("BAML_TELEMETRY", "medium");
        if index == 1 {
            command.arg("main");
        }
        run(&mut command, true);
        assert_recordings(&home, index + 1);
        run(command.env("BAML_TELEMETRY", "off"), true);
        assert_recordings(&home, index + 1);
    }
    assert!(!source.join(RECORDINGS).exists());
    assert!(!launch.join(RECORDINGS).exists());
    assert!(!temp.path().join(RECORDINGS).exists());
}

/// Sends one request whose body and response hold secrets. Neither secret is
/// a literal of the program, so only a recorded body can hold one.
const NETWORK_PROGRAM: &str = r#"
class Canned {
    server: baml.http.Server,

    function handle(self, req: baml.http.ServerRequest) -> baml.http.Response {
        let token = "response-" + "secret";
        baml.http.Response.new(200, { "content-type": "text/plain" }, token.to_utf8())
    }
}

function main() -> bool {
    let canned = Canned { server: baml.http.Server.bind("127.0.0.1:0") };
    let task = spawn { canned.server.serve(canned.handle) };
    let text = baml.http.send(baml.http.Request {
        method: "POST",
        url: "http://" + canned.server.addr + "/session",
        headers: { "content-type": "text/plain" },
        body: "request-" + "secret",
    }).text();
    let _ = task.cancel();
    text == "response-" + "secret"
}
"#;

/// `[pack] http_bodies = false` keeps request and response bodies out of a
/// packed binary's network spans, even at `high`; the request's method, the
/// response's status and the span's timing are still recorded.
#[test]
fn packed_http_bodies_false_keeps_bodies_out_of_network_spans() {
    let _built = common::ensure_built();
    for record_bodies in [true, false] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        std::fs::create_dir(&source).unwrap();
        common::write_project(&source, NETWORK_PROGRAM);
        std::fs::OpenOptions::new()
            .append(true)
            .open(source.join("baml.toml"))
            .unwrap()
            .write_all(
                format!(
                    "[pack]\n{}\n[pack.env_var_names]\nBAML_TELEMETRY = \"BAML_TELEMETRY\"\n",
                    if record_bodies {
                        ""
                    } else {
                        "http_bodies = false"
                    }
                )
                .as_bytes(),
            )
            .unwrap();
        let home = home(temp.path());
        common::share_build_cache(&home.join(".baml"));
        let binary = temp.path().join("packed");
        run(
            cli(temp.path(), &home)
                .args(["pack", "--from"])
                .arg(&source)
                .arg("-o")
                .arg(&binary)
                .arg("main"),
            true,
        );
        let output = Command::new(&binary)
            .current_dir(temp.path())
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("BAML_HOME", home.join(".baml"))
            .env_remove("BOUNDARY_API_KEY")
            .env("BAML_TELEMETRY", "high")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "true\n");
        assert_recordings(&home, 1);

        // Neither secret reached the recording or its CAS.
        let mut files = Vec::new();
        files_under(&home.join(".baml/btel"), &mut files);
        for secret in ["request-secret", "response-secret"] {
            let holding = files.iter().any(|file| {
                std::fs::read(file)
                    .unwrap()
                    .windows(secret.len())
                    .any(|window| window == secret.as_bytes())
            });
            assert_eq!(holding, record_bodies, "{secret}");
        }

        let output = cli(temp.path(), &home)
            .args(["query", "--local", "--format", "json", "--from"])
            .arg(&home)
            .arg(
                "SELECT input_args['request']['method'], input_args['request']['body'],
                   network_event_values[0]['payload']['status'],
                   network_event_values[1]['event_name'], network_event_values[1]['payload'],
                   duration > 0
                 FROM spans WHERE span_type = 'network_span'",
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let rows: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let body = |secret: &'static str| record_bodies.then_some(secret);
        assert_eq!(
            rows["rows"],
            serde_json::json!([[
                "POST",
                body("request-secret"),
                200,
                "data",
                body("response-secret"),
                1
            ]])
        );
    }
}

fn files_under(directory: &Path, found: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files_under(&path, found);
        } else {
            found.push(path);
        }
    }
}
