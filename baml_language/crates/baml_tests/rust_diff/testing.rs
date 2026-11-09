//! Exercise the actual packed-test host, including discovery and failure text.
use super::*;

pub fn run() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent().unwrap().parent().unwrap();
    let build = Path::new(env!("CARGO_TARGET_TMPDIR")).join("rt-testing-parity");
    let source = build.join("source");
    std::fs::create_dir_all(source.join("baml_src")).unwrap();
    std::fs::write(source.join("baml.toml"), include_str!("testing/baml.toml")).unwrap();
    std::fs::write(
        source.join("baml_src/main.baml"),
        include_str!("testing/main.baml"),
    )
    .unwrap();
    let pack_tests = |rt: bool| {
        let executable = build.join(if rt { "rt" } else { "vm" });
        if executable.exists() {
            std::fs::remove_file(&executable).unwrap();
        }
        let mut argv: Vec<String> = [
            "baml",
            "--agent-skill-check",
            "off",
            "pack",
            "--tests",
            "--rust",
            "--rust-profile",
            "fast",
            "--rust-workspace",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        argv.extend([
            workspace.display().to_string(),
            "--rust-build-dir".into(),
            build
                .join(if rt { "rt-build" } else { "vm-build" })
                .display()
                .to_string(),
            "--from".into(),
            source.display().to_string(),
            "-o".into(),
            executable.display().to_string(),
        ]);
        if rt {
            argv.push("--rt".into());
        }
        assert!(matches!(
            baml_cli::run_cli(argv).unwrap(),
            baml_cli::ExitCode::Success
        ));
        assert!(executable.is_file());
        executable
    };
    let native = pack_tests(true);
    let vm = pack_tests(false);
    let listing = run_probe(
        Command::new(&native).arg("--list"),
        Duration::from_secs(120),
    )
    .unwrap();
    assert!(listing.status.success());
    let inventory: Vec<serde_json::Value> = serde_json::from_slice(&listing.stdout).unwrap();
    assert_eq!(inventory.len(), 16, "complete standalone test discovery");
    for row in inventory {
        assert!(row["admission"].is_null(), "unexpected rejection: {row}");
        let name = row["name"].as_str().unwrap();
        for level in ["off", "low", "medium", "high", "default"] {
            let invoke = |rt: bool, stress: bool| {
                let home = tempfile::tempdir_in(&build).unwrap();
                let mut cmd = Command::new(if rt { &native } else { &vm });
                if rt {
                    cmd.args(["--exact-test", name]);
                } else {
                    cmd.args(["-i", name, "--max-concurrency", "1"]);
                }
                cmd.env("BAML_HOME", home.path())
                    .env("BAML_TELEMETRY", level)
                    .env("BAML_COMPILED_BODIES", "0")
                    .env("BAML_GC_STRESS", if stress { "1" } else { "0" })
                    .env("NO_COLOR", "1")
                    .env_remove("BOUNDARY_PROJECT")
                    .env_remove("BOUNDARY_API_URL");
                if level == "default" {
                    cmd.env_remove("BAML_TELEMETRY");
                }
                run_probe(&mut cmd, Duration::from_secs(120)).unwrap()
            };
            let expected = invoke(false, false);
            for stress in [false, true] {
                let actual = invoke(true, stress);
                let label = format!("{name}/{level}/stress={stress}");
                assert_eq!(
                    actual.status.code(),
                    expected.status.code(),
                    "{label}: status"
                );
                assert_eq!(actual.stderr, expected.stderr, "{label}: stderr");
                let stdout = String::from_utf8(actual.stdout).unwrap();
                let (program_output, report) = stdout
                    .rsplit_once("BAML_CORPUS_RESULT\t")
                    .expect("native test report");
                let report: serde_json::Value = serde_json::from_str(report.trim_end()).unwrap();
                assert_eq!(report["name"], name, "{label}: identity");
                let passed = report["outcome"] == "pass";
                assert_eq!(passed, expected.status.success(), "{label}: outcome");
                let verdict = if passed { "PASS" } else { "FAIL" };
                let mut rendered = format!("{program_output}{verdict} {name}\n");
                if let Some(message) = report["message"].as_str() {
                    rendered.push_str(&format!("  => {message}\n"));
                } else {
                    assert!(passed, "{label}: failure must have a message");
                }
                rendered.push_str(&format!(
                    "AGGREGATE {verdict} [outcome={}] {} passed, {} failed, 0 tolerated, 1 total\n",
                    if passed { "pass" } else { "fail" },
                    usize::from(passed),
                    usize::from(!passed)
                ));
                let oracle = String::from_utf8_lossy(&expected.stdout);
                assert_eq!(rendered, oracle, "{label}: test report");
            }
        }
    }
}
