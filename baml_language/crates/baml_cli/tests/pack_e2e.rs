// End-to-end tests for `baml pack`.
//
// Scope: the pack → embed → run pipeline, and only that. Everything the
// packaged binary does once it's running (auto-CLI parsing, output
// formatting, JSON coercion, target resolution with scripts, ...)
// shares code with `baml run` and is covered by unit tests in
// `run_command::tests` and `pack_command::tests`. Re-running those
// assertions through a subprocess would just slow the suite down.
//
// What stays e2e:
//   - The envelope actually round-trips (pack writes, host reads).
//   - The host actually dispatches and produces stdout.
//   - The target-identifier / `.baml` hermetic / `--function` paths
//     each reach a working binary.
//   - The baked-in `output-format` is honored, not ignored.
//   - `log.*` reaches stdio only when `BAML_LOG` asks, and then on stderr.
//   - `baml.sys.exit(n)` crosses the process boundary and becomes the
//     shell exit code.
//
// See `tests/common/mod.rs` for the HACK note on how binaries get
// discovered (TL;DR: we shell out to `cargo build` until artifact deps
// stabilize).

mod common;

use std::{
    io::{BufRead as _, BufReader, Write as _},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

use common::BuiltPaths;
use sys_native::SysOpsExt as _;

// ============================================================================
// Helpers
// ============================================================================

/// Variables a packed program reads that the tests set on purpose and nowhere
/// else. CI sets `BAML_LOG` for other jobs.
const CONTROLLED_VARS: [&str; 5] = [
    "BAML_GC_GROWTH",
    "BAML_GC_MIN_GROWTH",
    "ACME_GC_GROWTH",
    "BAML_LOG",
    "ACME_LOG",
];

/// A command that does not inherit the developer's or CI's values for
/// [`CONTROLLED_VARS`], so neither `baml pack` nor a packed program sees a
/// value a test did not set.
fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    for name in CONTROLLED_VARS {
        command.env_remove(name);
    }
    command
}

fn pack(built: &BuiltPaths, dir: &Path, pack_args: &[&str]) -> PathBuf {
    let out_bin = dir.join("out");
    let mut cmd = command(&built.baml_cli);
    cmd.arg("pack")
        .arg("--project-dir")
        .arg(dir)
        .arg("-o")
        .arg(&out_bin);
    // Share the build cache across the suite so only the first invocation
    // pays the stdlib compile; see `common::shared_baml_home`.
    cmd.env("BAML_HOME", common::shared_baml_home());
    // Ignore inherited agent env (CLAUDECODE/AI_AGENT/…): it would make the
    // agent-skill check apply.
    cmd.env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1");
    for arg in pack_args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("spawn baml-cli pack");
    assert!(
        output.status.success(),
        "pack failed: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(out_bin.exists(), "packed binary not produced");
    out_bin
}

fn run(binary: &Path, args: &[&str]) -> Output {
    let mut cmd = command(binary);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.output().expect("spawn packed binary")
}

fn pack_project(
    built: &BuiltPaths,
    source: &str,
    pack_args: &[&str],
) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    common::write_project(tmp.path(), source);
    let bin = pack(built, tmp.path(), pack_args);
    (tmp, bin)
}

// ============================================================================
// Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn json_envelope_reports_model_usage_for_run_and_pack() {
    use std::{
        sync::{Mutex, mpsc},
        time::Duration,
    };

    use serde_json::{Value, json};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::path};

    let built = common::ensure_built();
    let server = MockServer::start().await;
    let answer = json!({
        "model": "reported-model",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"}, "finish_reason": "stop"}],
    });
    let mut with_usage = answer.clone();
    with_usage["usage"] = json!({"prompt_tokens": 4111, "completion_tokens": 4222});
    Mock::given(path("/metered/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(with_usage.clone()))
        .expect(6)
        .mount(&server)
        .await;
    Mock::given(path("/unmetered/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer))
        .expect(1)
        .mount(&server)
        .await;
    let (release_response, response_gate) = mpsc::channel();
    let response_gate = Mutex::new(response_gate);
    Mock::given(path("/background/chat/completions"))
        .respond_with(move |_: &Request| {
            response_gate
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(30))
                .unwrap();
            ResponseTemplate::new(200).set_body_json(&with_usage)
        })
        .expect(1)
        .mount(&server)
        .await;

    let source = format!(
        r#"
function Metered(x: string) -> string {{
    client: openai.GenericClient.new(model = "requested-model", base_url = "{uri}/metered", api_key = "test", capture_wire = false)
    prompt: `Echo ${{x}}`
}}

function Unmetered(x: string) -> string {{
    client: openai.GenericClient.new(model = "requested-model", base_url = "{uri}/unmetered", api_key = "test", capture_wire = false)
    prompt: `Echo ${{x}}`
}}

function Twice(x: string) -> string {{
    let pending = spawn {{ Metered(x) }};
    let _ = Metered(x);
    await pending
}}

function BackgroundModel(x: string) -> string {{
    client: openai.GenericClient.new(model = "requested-model", base_url = "{uri}/background", api_key = "test", capture_wire = false)
    prompt: `Echo ${{x}}`
}}

function Background(x: string) -> string {{
    let value = Metered(x);
    let _ = spawn {{ BackgroundModel(x) }};
    value
}}
"#,
        uri = server.uri()
    );
    let (project, binary) = pack_project(built, &source, &["Metered"]);

    let spawn = |cmd: &mut Command| {
        let mut child = cmd
            .args(["--json-args", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(br#"{"x":"hi"}"#)
            .unwrap();
        child
    };
    let invoke = |cmd: &mut Command| -> Value {
        let child = spawn(cmd);
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice(&result.stdout).unwrap()
    };

    let run_command = |target: &str, format: &str| {
        let mut cmd = command(&built.baml_cli);
        cmd.current_dir(project.path())
            .env("BAML_HOME", common::shared_baml_home())
            .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
            .env("BAML_TELEMETRY", "off")
            .args(["run", "--output-format", format, "-f", target, "--", target]);
        cmd
    };
    let run = |target, format| invoke(&mut run_command(target, format));
    let expected = |calls: u64| {
        json!({
            "value": "ok",
            "usage": [{
                "provider": "openai-generic",
                "model": "reported-model",
                "calls": calls,
                "input_tokens": 4111 * calls,
                "output_tokens": 4222 * calls,
                "cache_read_tokens": null,
                "cache_write_tokens": null,
                "reasoning_tokens": null,
            }],
        })
    };
    assert_eq!(run("Metered", "json"), json!("ok"));
    assert_eq!(run("Metered", "json-envelope"), expected(1));
    assert_eq!(run("Twice", "json-envelope"), expected(2));
    assert_eq!(
        run("Unmetered", "json-envelope"),
        json!({"value": "ok", "usage": []})
    );
    assert_eq!(
        invoke(&mut command(&binary).args(["--output-format", "json-envelope"])),
        expected(1),
    );

    // Hold the background model's response until the CLI has emitted the envelope.
    let mut child = spawn(&mut run_command("Background", "json-envelope"));
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    release_response.send(()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_str::<Value>(&line).unwrap(),
        json!({"value": "ok", "usage": null})
    );
}

/// The application has one version regardless of transport. Exercise source
/// execution (including a warm cache), a real packed host, and generated SDK
/// bytecode through the runtime that bridges use, all before package `$init`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn package_version_reaches_source_pack_and_bridge_before_initialization() {
    let built = common::ensure_built();
    let project = tempfile::tempdir().unwrap();
    let source = format!(
        r#"
function version(key: string) -> string {{
    match (trace.current_context().metadata.get(key)) {{
        let value: string => value,
        _ => "<unset>",
    }}
}}
const initial_baml_version = version("baml.ctx.baml_version");
const initial_app_version = version("baml.ctx.app_version");
function main() -> string {{
    assert.equal(initial_baml_version, {baml_version:?});
    assert.equal(version("baml.ctx.baml_version"), initial_baml_version);
    assert.equal(version("baml.ctx.app_version"), initial_app_version);
    initial_app_version
}}
"#,
        baml_version = baml_version::CANONICAL_VERSION
    );
    common::write_project(project.path(), &source);
    let cli = |args: &[&str]| {
        command(&built.baml_cli)
            .args(args)
            .current_dir(project.path())
            .env("BAML_HOME", common::shared_baml_home())
            .env("BAML_CLI_ALLOW_DIRECT", "1")
            .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
            .env("DO_NOT_TRACK", "1")
            .env("BAML_TELEMETRY", "off")
            .env("BOUNDARY_API_KEY", "local")
            .env_remove("BAML_CONTEXT")
            .output()
            .unwrap()
    };
    for version in [
        None,
        Some("1.2.3+build.4"),
        Some("2.0.0-beta.1"),
        Some(""),
        Some(" \t "),
        Some(" 3.4.5+build.6 "),
        Some("build-canary"),
        None,
    ] {
        let setting = version
            .map(|value| format!("version = {value:?}\n"))
            .unwrap_or_default();
        std::fs::write(
            project.path().join("baml.toml"),
            format!(
                r#"[package]
name = "version-context"
{setting}
[pack.env_var_names]
BAML_TELEMETRY = "BAML_TELEMETRY"
[generator.rust]
output_type = "rust"
output_dir = "generated"
naming_convention = "preserve-case"
"#
            ),
        )
        .unwrap();
        let expected = version
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("<unset>");
        for _ in 0..2 {
            let output = cli(&["run", "main", "--output-format", "json"]);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                serde_json::from_slice::<String>(&output.stdout).unwrap(),
                expected
            );
        }
        let packed = cli(&["pack", "main", "--output", "app"]);
        assert!(
            packed.status.success(),
            "{}",
            String::from_utf8_lossy(&packed.stderr)
        );
        let output = command(project.path().join("app"))
            .env("BAML_HOME", project.path().join("packed-home"))
            .env("BOUNDARY_API_KEY", "local")
            .env("DO_NOT_TRACK", "1")
            .env("BAML_TELEMETRY", "off")
            .env(
                "BAML_CONTEXT",
                if expected != "<unset>" {
                    r#"{"metadata":{"baml.ctx.baml_version":"spoofed","baml.ctx.app_version":"spoofed"}}"#
                } else {
                    r#"{"metadata":{"baml.ctx.baml_version":"spoofed"}}"#
                },
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<String>(&output.stdout).unwrap(),
            expected
        );

        let generated = cli(&["generate"]);
        assert!(
            generated.status.success(),
            "{}",
            String::from_utf8_lossy(&generated.stderr)
        );
        let bytes = std::fs::read(
            project
                .path()
                .join("generated/baml_sdk/src/_inlinedbaml.bin"),
        )
        .unwrap();
        let runtime = bex_project::prepare_from_bytecode(&bytes, sys_native::SysOps::native())
            .unwrap()
            .with_artifact_telemetry(btel_settings::artifact::ArtifactTelemetry {
                recording_level: btel_settings::artifact::RecordingLevel::Off,
                ..Default::default()
            })
            .build()
            .unwrap();
        let result = runtime
            .call_function(
                "main",
                vec![],
                bex_engine::FunctionCallContextBuilder::new(bex_engine::CallId::next()).build(),
                true,
            )
            .await
            .unwrap();
        assert_eq!(
            result,
            bex_engine::BexExternalValue::String(expected.into())
        );
        runtime.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn artifact_recording_level_requires_explicit_permission_and_off_skips_auth() {
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};
    let built = common::ensure_built();
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    let bin = pack(built, temp.path(), &["main"]);
    let output = command(&bin)
        .env("BAML_TELEMETRY", "invalid")
        .env("BOUNDARY_API_KEY", "local")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ambient BAML_TELEMETRY must not override the baked default: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    std::fs::write(
        temp.path().join("baml.toml"),
        r#"[package]
name = "test"
[pack.env_var_names]
BAML_TELEMETRY = "ACME_TELEMETRY"
"#,
    )
    .unwrap();
    let bin = pack(built, temp.path(), &["main"]);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401))
        .expect(0)
        .mount(&server)
        .await;
    let output = command(&bin)
        .env("BAML_TELEMETRY", "invalid")
        .env("ACME_TELEMETRY", "off")
        .env("BOUNDARY_API_KEY", "bdry_secret_test")
        .env("BOUNDARY_API_URL", server.uri())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "status: {}; stdout: {}; stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "7\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");

    // A value that is not a level never stops the program, and records nothing.
    for level in ["false", "invalid"] {
        let output = command(&bin)
            .env("ACME_TELEMETRY", level)
            .env("BOUNDARY_API_KEY", "bdry_secret_test")
            .env("BOUNDARY_API_URL", server.uri())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{level}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "7\n");
        assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn artifact_initial_failure_warning_is_customizable_and_ignore_is_silent() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path_regex},
    };
    let built = common::ensure_built();
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    for action in ["warn", "ignore"] {
        std::fs::write(
            temp.path().join("baml.toml"),
            format!(
                r#"[package]
name = "test"
[pack]
on_initial_telemetry_failure = "{action}"
initial_telemetry_warning_message = "Telemetry unavailable; continuing."
"#
            ),
        )
        .unwrap();
        let bin = pack(built, temp.path(), &["main"]);
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(r"/heartbeat$"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;
        let output = command(&bin)
            .env("BOUNDARY_API_KEY", "bdry_secret_test")
            .env("BOUNDARY_API_URL", server.uri())
            .env("BOUNDARY_PROJECT", "acme/app")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "7\n");
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            if action == "warn" {
                r#"warning: Telemetry unavailable; continuing.
"#
            } else {
                ""
            }
        );
    }
}

/// Pack root `main`, run it, observe its return value on stdout.
/// Validates the whole pipeline: envelope roundtrip, host dispatch,
/// output formatting, auto-CLI parameter binding — all together.
/// If this breaks, every other e2e test will too.
#[test]
fn pack_e2e_root_main() {
    let built = common::ensure_built();
    let (_tmp, bin) = pack_project(
        built,
        "function main(name: string) -> string { \"hi, \" + name }\n",
        &["main"],
    );
    let out = run(&bin, &["--name", "Ada"]);
    assert!(
        out.status.success(),
        "packed binary exited {:?}; stderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("hi, Ada"));
}

/// The packed envelope carries the root's enriched `PackageInterface`, and the
/// standalone host carries the transient runtime compiler. No source files are
/// available after packing: this is the end-to-end exit criterion for lexical
/// `Package.current()` plus alias-mounted `Package.compile`.
#[test]
fn pack_e2e_current_package_compiles_and_runs_skill() {
    let built = common::ensure_built();
    let source = r####"
class AgentState {
  goal string
  history string[]
}

interface AgentAction {
  summary string
}

function Plan(state: AgentState) -> string {
  "packed plan: " + state.goal
}

function main() -> string throws unknown {
  let skill = reflect.Package.compile(
    { "skill.baml": `
class PlanThenAct {
  summary string
  steps string[]
  implements app.AgentAction {}
}

function Run(state: app.AgentState) -> PlanThenAct {
  PlanThenAct { summary: app.Plan(state), steps: [] }
}
` },
    packages = { "app": reflect.Package.current() },
  )
  let run = skill.get_function<(AgentState) -> AgentAction>("root.Run")
  run(AgentState { goal: "binary", history: [] }).summary
}
"####;
    let (_tmp, bin) = pack_project(built, source, &["main"]);
    let out = run(&bin, &[]);
    assert!(
        out.status.success(),
        "packed runtime-compile binary exited {:?}; stdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("packed plan: binary"),
        "unexpected packed output: {}",
        String::from_utf8_lossy(&out.stdout),
    );
}

/// `baml pack` keeps packaging progress, but should not emit the compile
/// file-count status pair reserved for `check` and `generate`.
#[test]
fn pack_e2e_omits_compile_file_status() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    common::write_project(
        tmp.path(),
        "function main() -> string { \"packed quietly\" }\n",
    );
    let out_bin = tmp.path().join("out");

    let output = command(&built.baml_cli)
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        // Ignore inherited agent env (CLAUDECODE/AI_AGENT/…): it would flip
        // `--output-preset auto` to `agent` and hide progress lines.
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .env("BAML_HOME", common::shared_baml_home())
        .arg("pack")
        .arg("--project-dir")
        .arg(tmp.path())
        .arg("-o")
        .arg(&out_bin)
        .arg("main")
        .output()
        .expect("spawn baml-cli pack");

    assert!(
        output.status.success(),
        "pack failed: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    common::assert_no_compile_file_status(&String::from_utf8_lossy(&output.stderr));
}

/// `--function` produces a subcommand-mode binary even with a single
/// target. The function name becomes `argv[1]` and parameter flags
/// follow it — a different dispatch path than the positional `pack
/// <target>` form, which produces a no-subcommand binary.
#[test]
fn pack_e2e_function_target() {
    let built = common::ensure_built();
    let (_tmp, bin) = pack_project(
        built,
        "function Greet(who: string) -> string { \"hello \" + who }\n",
        &["--function", "Greet"],
    );
    let out = run(&bin, &["Greet", "--who", "World"]);
    assert!(
        out.status.success(),
        "packed binary exited {:?}; stderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("hello World"));
}

/// `--file <path>` takes the hermetic-load path — a different
/// project-loading code path from the project-based pack above.
#[test]
fn pack_e2e_hermetic_baml_file() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("hello.baml");
    std::fs::write(&src, "function main() -> string { \"hermetic\" }\n").unwrap();
    let out_bin = tmp.path().join("out");
    let status = command(&built.baml_cli)
        .env("BAML_HOME", common::shared_baml_home())
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .arg("pack")
        .arg("--file")
        .arg(&src)
        .arg("main")
        .arg("-o")
        .arg(&out_bin)
        .status()
        .expect("spawn baml-cli pack");
    assert!(status.success());
    let run_out = run(&out_bin, &[]);
    assert!(
        run_out.status.success(),
        "hermetic binary failed; stderr:\n{}",
        String::from_utf8_lossy(&run_out.stderr),
    );
    assert!(String::from_utf8_lossy(&run_out.stdout).contains("hermetic"));
}

/// A packed binary picks its output format at run time, like `baml run`:
/// text by default, `--output-format json` among a typed target's flags.
#[test]
fn pack_e2e_output_format_is_a_runtime_flag() {
    let built = common::ensure_built();
    let (_tmp, bin) = pack_project(
        built,
        "class Point {\n    x: int,\n}\n\nfunction main(x: int) -> Point {\n    Point { x: x }\n}\n",
        &["main"],
    );

    let text = run(&bin, &["--x", "42"]);
    assert!(text.status.success(), "{text:?}");
    let stdout = String::from_utf8(text.stdout).unwrap();
    assert!(stdout.contains("42"), "{stdout}");
    assert!(
        serde_json::from_str::<serde_json::Value>(stdout.trim()).is_err(),
        "the default is text, not JSON: {stdout}"
    );

    let json = run(&bin, &["--x", "42", "--output-format", "json"]);
    assert!(json.status.success(), "{json:?}");
    let parsed: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(parsed, serde_json::json!({"x": 42}));
}

/// A packed program's stdio is its own: its `log.*` events stay off it unless
/// `BAML_LOG` sets a level, and then they go to stderr, leaving stdout to the
/// program.
#[test]
fn pack_e2e_logs_stay_off_stdio_unless_baml_log_sets_a_level() {
    let built = common::ensure_built();
    let (_tmp, bin) = pack_project(
        built,
        "function main() -> string {\n    log.error(\"oops\");\n    log.info(\"hello\");\n    \"done\"\n}\n",
        &["main"],
    );
    let run_with = |level: Option<&str>| {
        let mut cmd = command(&bin);
        if let Some(level) = level {
            cmd.env("BAML_LOG", level);
        }
        cmd.output().expect("spawn packed binary")
    };
    let text = |bytes: Vec<u8>| String::from_utf8(bytes).unwrap();

    let silent = run_with(None);
    assert!(silent.status.success(), "{silent:?}");
    assert_eq!(text(silent.stdout).trim(), "\"done\"");
    let stderr = text(silent.stderr);
    assert!(
        !stderr.contains("oops") && !stderr.contains("hello"),
        "{stderr}"
    );

    let errors = run_with(Some("ERROR"));
    assert!(errors.status.success(), "{errors:?}");
    assert_eq!(text(errors.stdout).trim(), "\"done\"");
    let stderr = text(errors.stderr);
    assert!(stderr.contains("[ERROR] oops"), "{stderr}");
    assert!(!stderr.contains("hello"), "{stderr}");

    let info = run_with(Some("info"));
    assert!(info.status.success(), "{info:?}");
    assert_eq!(text(info.stdout).trim(), "\"done\"");
    let stderr = text(info.stderr);
    assert!(
        stderr.contains("[ERROR] oops") && stderr.contains("[INFO] hello"),
        "{stderr}"
    );

    let invalid = run_with(Some("loud"));
    assert!(!invalid.status.success(), "{invalid:?}");
    assert!(invalid.stdout.is_empty(), "{invalid:?}");
    let stderr = text(invalid.stderr);
    assert!(
        stderr.contains(
            r#"BAML_LOG must be one of off, error, warn, info, debug, trace, got "loud""#
        ),
        "{stderr}"
    );
}

/// A publisher renames the log variable as it does the GC ones: the program
/// reads only the name it carries.
#[test]
fn pack_e2e_publisher_renames_the_log_variable() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    common::write_project(
        tmp.path(),
        "function main() -> string {\n    log.info(\"hello\");\n    \"done\"\n}\n",
    );
    std::fs::write(
        tmp.path().join("baml.toml"),
        "[package]\nname = \"pack_e2e\"\n\n[pack.env_var_names]\nBAML_LOG = \"ACME_LOG\"\n",
    )
    .unwrap();
    let bin = pack(built, tmp.path(), &["main"]);
    let run_with = |vars: &[(&str, &str)]| {
        let mut cmd = command(&bin);
        cmd.envs(vars.iter().copied());
        cmd.output().expect("spawn packed binary")
    };
    let text = |bytes: Vec<u8>| String::from_utf8(bytes).unwrap();

    // The standard name, even an invalid value, is someone else's variable.
    for value in ["info", "loud"] {
        let ignored = run_with(&[("BAML_LOG", value)]);
        assert!(ignored.status.success(), "{value}: {ignored:?}");
        assert_eq!(text(ignored.stdout).trim(), "\"done\"");
        assert_eq!(text(ignored.stderr), "", "{value}");
    }

    let logged = run_with(&[("ACME_LOG", "info")]);
    assert!(logged.status.success(), "{logged:?}");
    assert_eq!(text(logged.stdout).trim(), "\"done\"");
    assert!(text(logged.stderr).contains("[INFO] hello"));

    let invalid = run_with(&[("ACME_LOG", "loud")]);
    assert!(!invalid.status.success(), "{invalid:?}");
    let stderr = text(invalid.stderr);
    assert!(
        stderr.contains(
            r#"ACME_LOG must be one of off, error, warn, info, debug, trace, got "loud""#
        ),
        "{stderr}"
    );
}

/// `baml.sys.exit(n)` must cross the subprocess boundary unchanged.
/// Exercises: `baml.panics.Exit` unwind → engine detects the class →
/// `EngineError::Exit` → `DispatchResult::Exit` → `clamp_exit_code`
/// → `std::process::exit(n)` → shell observes `n`.
#[test]
fn pack_e2e_sys_exit_propagates_to_shell() {
    let built = common::ensure_built();
    let (_tmp, bin) = pack_project(
        built,
        "function main() -> never {\n  baml.sys.exit(42)\n}\n",
        &["main"],
    );
    let out = run(&bin, &[]);
    assert_eq!(out.status.code(), Some(42));
}

/// Pack a **manifest-less** `baml_src/`-only project (no `baml.toml`) and
/// run the resulting binary. Proves the whole pack pipeline works without a
/// manifest; `-o` overrides the artifact path, so the dir-name fallback in
/// `resolve_project_name` is covered separately by the unit tests.
#[test]
fn pack_e2e_manifest_less_baml_src() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    // No baml.toml — sources live under baml_src/.
    let src = tmp.path().join("baml_src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("main.baml"),
        "function main(name: string) -> string { \"hi, \" + name }\n",
    )
    .unwrap();

    let bin = pack(built, tmp.path(), &["main"]);
    let out = run(&bin, &["--name", "Ada"]);
    assert!(
        out.status.success(),
        "manifest-less packed binary exited {:?}; stderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("hi, Ada"));
}

fn run_with_context(binary: &Path, args: &[&str], context: Option<&str>, stdin: &str) -> Output {
    use std::io::Write as _;
    let mut cmd = command(binary);
    cmd.args(args)
        .env_remove("BAML_CONTEXT")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(context) = context {
        cmd.env("BAML_CONTEXT", context);
    }
    let mut child = cmd.spawn().expect("spawn packed binary");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

/// `BAML_CONTEXT` is the packed-binary equivalent of `baml run --context`:
/// same sources, same shape, same stdin rules, in both dispatch modes.
#[test]
fn pack_e2e_baml_context_env() {
    const CONTEXT: &str = r#"{"distinct_id":"pipe-user","metadata":{"phase":"launch"}}"#;
    let built = common::ensure_built();
    let source = include_str!("fixtures/launch_context/baml_src/main.baml");
    let (tmp, single) = pack_project(built, source, &["Check"]);
    let (_multi_tmp, multi) = pack_project(built, source, &["--function", "Check"]);
    let context_file = tmp.path().join("context.json");
    std::fs::write(&context_file, CONTEXT).unwrap();
    let at_file = format!("@{}", context_file.display());

    let succeeds = |out: &Output, expected: &str| {
        assert!(
            out.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), expected);
    };
    for (bin, prefix) in [(&single, &[][..]), (&multi, &["Check"][..])] {
        let args = |rest: &[&'static str]| -> Vec<&'static str> {
            prefix.iter().chain(rest).copied().collect()
        };
        for context in [CONTEXT, at_file.as_str()] {
            succeeds(
                &run_with_context(bin, &args(&["--value", "x"]), Some(context), ""),
                r#""x""#,
            );
        }
        succeeds(
            &run_with_context(bin, &args(&["--value", "x"]), Some("-"), CONTEXT),
            r#""x""#,
        );
        succeeds(
            &run_with_context(
                bin,
                &args(&["--json-args", "-"]),
                Some(CONTEXT),
                r#"{"value":"json"}"#,
            ),
            r#""json""#,
        );

        // Help never reads the context, so an empty stdin is fine.
        let out = run_with_context(bin, &args(&["--help"]), Some("-"), "");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );

        let out = run_with_context(bin, &args(&["--json-args", "-"]), Some("-"), CONTEXT);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("only one stdin consumer"),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );

        // Set but empty is a source, as with clap `env` flags on `baml run`.
        for context in ["[]", ""] {
            let out = run_with_context(bin, &args(&["--value", "x"]), Some(context), "");
            assert!(!out.status.success(), "{context:?}");
            assert!(out.stdout.is_empty());
            assert!(String::from_utf8_lossy(&out.stderr).contains("invalid BAML_CONTEXT"));
        }

        // Without a context the fixture's assertions fail.
        let out = run_with_context(bin, &args(&["--value", "x"]), None, "");
        assert!(!out.status.success());
        assert!(!String::from_utf8_lossy(&out.stderr).contains("BAML_CONTEXT"));
    }
}

#[test]
#[ignore = "builds a generated Rust crate with cargo"]
fn rust_pack_from_outside_workspace_with_cargo_overrides() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let project = tempfile::tempdir().unwrap();
    common::write_project(project.path(), "function main() -> int { 7 }\n");
    let caller = tempfile::tempdir().unwrap();
    let output_path = caller
        .path()
        .join(format!("packed{}", std::env::consts::EXE_SUFFIX));
    for explicit_build_dir in [false, true] {
        let mut pack = command(common::baml_cli());
        pack.current_dir(caller.path())
            .args(["pack", "main", "--rust", "--rust-profile", "fast"])
            .arg("--rust-workspace")
            .arg(workspace)
            .arg("--project-dir")
            .arg(project.path())
            .arg("--output")
            .arg(&output_path)
            .env("BAML_HOME", common::shared_baml_home())
            .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
            .env("CARGO_BUILD_TARGET", "wasm32-unknown-unknown")
            .env("RUSTUP_TOOLCHAIN", "missing-baml-test-toolchain")
            .env("CARGO_TARGET_DIR", caller.path().join("wrong-target"))
            .env("CARGO_PROFILE_RELEASE_OPT_LEVEL", "invalid")
            .env("CARGO_PROFILE_RELEASE_STRIP", "invalid");
        if explicit_build_dir {
            pack.args(["--rust-build-dir", "target/baml-rust"]);
        }
        assert!(pack.status().expect("spawn Rust pack").success());
        assert!(caller.path().join("target/baml-rust/Cargo.toml").is_file());
        let output = run(&output_path, &[]);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "7");
    }
}

/// A packed host launches an ordinary child with inherited streams and forwards
/// its exit status after normal runtime shutdown.
#[test]
#[cfg(unix)]
fn pack_child_preserves_streams_environment_and_status() {
    use std::{io::Write as _, process::Stdio};
    let built = common::ensure_built();
    let (_tmp, bin) = pack_project(
        built,
        r#"
        function main() -> never {
            let status = baml.sys.run("sh", args = ["-c", "printf '%s:%s:%s:' $$ \"$BAML_HANDOFF\" \"${BAML_HANDOFF_REMOVE-unset}\"; cat; printf err >&2; exit 23"], env = map { "BAML_HANDOFF": "overlay", "BAML_HANDOFF_REMOVE": null });
            baml.sys.exit(status.exit_code)
        }
    "#,
        &["main"],
    );
    let mut child = command(bin)
        .env("BAML_HANDOFF_REMOVE", "inherited")
        // A saved Boundary login on the machine would add a warning to stderr.
        .env("BAML_TELEMETRY", "off")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    child.stdin.take().unwrap().write_all(b"input").unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(23));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let (child_pid, rest) = stdout.split_once(':').unwrap();
    assert_ne!(child_pid.parse::<u32>().unwrap(), pid);
    assert_eq!(rest, "overlay:unset:input");
    assert_eq!(output.stderr, b"err");
}

#[test]
#[cfg(windows)]
fn pack_child_preserves_full_windows_exit_status() {
    let built = common::ensure_built();
    let (_tmp, bin) = pack_project(
        built,
        r#"function main() -> never { let status = baml.sys.run("cmd", args = ["/c", "exit -23"]); baml.sys.exit(status.exit_code) }"#,
        &["main"],
    );
    assert_eq!(run(&bin, &[]).status.code(), Some(-23));
}

/// Runtime shutdown drops its reaper/handles without terminating an OS child.
#[test]
#[cfg(unix)]
fn pack_child_survives_cleanup_and_runtime_shutdown() {
    let built = common::ensure_built();
    let (tmp, bin) = pack_project(
        built,
        r#"
        function main(marker: string) -> void {
            let child = baml.sys.subprocess("sh",
                args = ["-c", "sleep 5; printf completed > \"$1\"", "sh", marker],
                stdin = "ignore", stdout = "ignore", stderr = "ignore");
            child.cleanup();
            baml.sys.collect_garbage();
        }
        "#,
        &["main"],
    );
    let marker = tmp.path().join("completed");
    let output = command(bin).arg("--marker").arg(&marker).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !marker.exists(),
        "the host must exit while its child is still running"
    );
    // The shell creates the file before `printf` writes to it, so wait for
    // the text and not for the file.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    let written = || std::fs::read_to_string(&marker).unwrap_or_default();
    while written() != "completed" && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(written(), "completed");
}

/// `[pack.gc]` and its variable names reach a real packed binary: the program
/// runs with the baked policy, reads only the variables the artifact carries
/// (a renamed one replaces the standard name), and refuses a bad value at
/// startup. `baml run` reads the standard variables and ignores `[pack.gc]`.
#[test]
fn pack_e2e_gc_settings_are_baked_and_overridable() {
    let built = common::ensure_built();
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    let manifest = |gc: &str| {
        std::fs::write(
            temp.path().join("baml.toml"),
            format!("[package]\nname = \"test\"\n{gc}"),
        )
        .unwrap();
    };
    let run_with =
        |bin: &Path, env: &[(&str, &str)]| command(bin).envs(env.iter().copied()).output().unwrap();

    // A setting that cannot work fails the pack, naming the key.
    for bad in [
        "[pack.gc]\ngrowth = 0.1",
        "[pack.gc]\nmin_growth = \"32MB\"",
        "[pack.gc]\nmin_budget = \"8MiB\"",
        "[bridge.gc]\ngrowth = 2",
    ] {
        manifest(bad);
        let output = command(&built.baml_cli)
            .args(["pack", "--project-dir"])
            .arg(temp.path())
            .arg("-o")
            .arg(temp.path().join("bad"))
            .arg("main")
            .env("BAML_HOME", common::shared_baml_home())
            .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{bad} packed");
        assert!(stderr.contains(".gc"), "{bad}: {stderr}");
    }

    // `baml pack` runs no target, so the packer's own GC variables, even bad
    // ones, neither fail the pack nor reach the binary.
    manifest("");
    let packed = temp.path().join("packed_with_env");
    let output = command(&built.baml_cli)
        .args(["pack", "--project-dir"])
        .arg(temp.path())
        .arg("-o")
        .arg(&packed)
        .arg("main")
        .env("BAML_HOME", common::shared_baml_home())
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .env("BAML_GC_GROWTH", "fast")
        .env("BAML_GC_MIN_GROWTH", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "pack read the packer's GC variables: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = run_with(&packed, &[]);
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "7\n");

    manifest(
        "[pack.gc]\ngrowth = 0.5\nmin_growth = \"2MiB\"\n[pack.env_var_names]\nBAML_GC_GROWTH = \"ACME_GC_GROWTH\"\n",
    );
    let bin = pack(built, temp.path(), &["main"]);
    let ok = |output: &Output, what: &str| {
        assert!(
            output.status.success(),
            "{what}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "7\n", "{what}");
    };
    ok(&run_with(&bin, &[]), "baked policy");
    ok(
        &run_with(&bin, &[("BAML_GC_GROWTH", "not a number")]),
        "the renamed standard variable is not read",
    );
    ok(
        &run_with(
            &bin,
            &[("ACME_GC_GROWTH", "8"), ("BAML_GC_MIN_GROWTH", "64MiB")],
        ),
        "valid overrides",
    );
    for (name, value) in [("ACME_GC_GROWTH", "0.1"), ("BAML_GC_MIN_GROWTH", "0")] {
        let output = run_with(&bin, &[(name, value)]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{name}={value} ran");
        assert!(stderr.contains(name), "{name}={value}: {stderr}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "", "nothing ran");
    }

    // Source runs use the standard names; `[pack.gc]` does not apply there.
    let baml_run = |env: &[(&str, &str)]| {
        command(&built.baml_cli)
            .args(["run", "main"])
            .current_dir(temp.path())
            .env("BAML_HOME", common::shared_baml_home())
            .env("BAML_CLI_ALLOW_DIRECT", "1")
            .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
            .env("BAML_TELEMETRY", "off")
            .envs(env.iter().copied())
            .output()
            .unwrap()
    };
    for (env, what) in [
        (("ACME_GC_GROWTH", "0.1"), "baml run ignores pack names"),
        (("BAML_GC_GROWTH", "2.5"), "baml run with growth"),
    ] {
        let output = baml_run(&[env]);
        assert!(
            output.status.success(),
            "{what}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = baml_run(&[("BAML_GC_GROWTH", "fast")]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("BAML_GC_GROWTH"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The baked policy reaches the collector of a real packed binary, through
/// the host's envelope-to-engine glue: counted as `baml.gc` telemetry spans,
/// `[pack.gc]` and the same values from the environment collect equally often,
/// and more often than the defaults.
#[test]
fn pack_e2e_gc_settings_reach_the_collector() {
    const SOURCE: &str = r#"
class Node { left: Node?, right: Node? }
function tree(depth: int) -> Node {
  if (depth <= 0) { return Node { left: null, right: null }; }
  Node { left: tree(depth - 1), right: tree(depth - 1) }
}
function count(node: Node) -> int {
  let l = node.left;
  let r = node.right;
  if (l == null || r == null) { return 1; }
  1 + count(l) + count(r)
}
function main() -> int {
  let total = 0;
  for (let i = 0; i < 400; i += 1) { total += count(tree(10)); }
  total
}
"#;
    let built = common::ensure_built();
    let temp = tempfile::tempdir().unwrap();
    let pack_with = |name: &str, gc: &str| {
        let dir = temp.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        common::write_project(&dir, SOURCE);
        std::fs::write(
            dir.join("baml.toml"),
            format!(
                "[package]\nname = \"gc_probe\"\n[pack.env_var_names]\nBAML_TELEMETRY = \"BAML_TELEMETRY\"\n{gc}"
            ),
        )
        .unwrap();
        pack(built, &dir, &["main"])
    };
    // One run in a fresh project, recording at `low`; its collections, less
    // the one at shutdown.
    let mut runs = 0;
    let mut collections = |bin: &Path, env: &[(&str, &str)]| {
        runs += 1;
        let dir = temp.path().join(format!("run{runs}"));
        std::fs::create_dir_all(dir.join("baml_src")).unwrap();
        std::fs::write(dir.join("baml.toml"), "[package]\nname = \"run\"\n").unwrap();
        let home = dir.join(".baml");
        let output = command(bin)
            .current_dir(&dir)
            .env("BAML_HOME", &home)
            .env("BAML_TELEMETRY", "low")
            .env_remove("BOUNDARY_API_KEY")
            .env_remove("BOUNDARY_PROJECT")
            .envs(env.iter().copied())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "818800\n");
        let query = command(&built.baml_cli)
            .current_dir(&dir)
            .env("BAML_HOME", &home)
            .env("BAML_CLI_ALLOW_DIRECT", "1")
            .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
            .args([
                "--color",
                "never",
                "--output-format",
                "json",
                "local",
                "recordings",
                "query",
            ])
            .arg("SELECT context_metadata FROM spans WHERE span_name = 'baml.gc'")
            .output()
            .unwrap();
        assert!(
            query.status.success(),
            "{}",
            String::from_utf8_lossy(&query.stderr)
        );
        let rows: serde_json::Value = serde_json::from_slice(&query.stdout).unwrap();
        rows["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row[0]["trigger"] != "shutdown")
            .count()
    };

    let defaults = pack_with("defaults", "");
    let baked = pack_with("baked", "[pack.gc]\ngrowth = 2\nmin_growth = \"1MiB\"\n");
    let at_defaults = collections(&defaults, &[]);
    let from_manifest = collections(&baked, &[]);
    let from_env = collections(
        &defaults,
        &[("BAML_GC_GROWTH", "2"), ("BAML_GC_MIN_GROWTH", "1MiB")],
    );
    assert_eq!(
        from_manifest, from_env,
        "the same policy, baked or from the environment"
    );
    assert!(
        from_manifest > at_defaults,
        "[pack.gc] {from_manifest} vs defaults {at_defaults} collections"
    );
}
