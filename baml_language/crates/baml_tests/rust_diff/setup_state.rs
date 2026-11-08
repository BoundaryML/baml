//! Setup state survives repeated jobs and collecting calls on one root fiber.
use super::*;

pub(super) fn run() {
    run_case(false);
    run_case(true);
}

fn run_case(string_result: bool) {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let build = Path::new(env!("CARGO_TARGET_TMPDIR")).join("rust-differential");
    let source = build.join("setup-state-source");
    std::fs::create_dir_all(source.join("baml_src")).unwrap();
    std::fs::write(
        source.join("baml.toml"),
        r#"[package]
name = "setup-state"
[pack.env_var_names]
BAML_TELEMETRY = "BAML_TELEMETRY"
"#,
    )
    .unwrap();
    let input = include_str!("setup_state.baml");
    let input = if string_result {
        input
            .replace("-> int {", "-> string {")
            .replace("777", "\"hello\\n\\\"world\\\"\"")
            .replace(
                "state.payload.length() + state.stamp + sum % 7",
                "(state.payload.length() + state.stamp + sum % 7).to_string()",
            )
    } else {
        input.to_owned()
    };
    std::fs::write(source.join("baml_src/main.baml"), &input).unwrap();
    let executable = build.join("setup-state-benchmark");
    pack_result_options(
        &build,
        workspace,
        &source,
        &executable,
        ["main"],
        true,
        true,
    )
    .unwrap();
    for level in ["off", "medium"] {
        for stress in ["0", "1"] {
            let home = tempfile::tempdir().unwrap();
            let mut command = Command::new(&executable);
            command
                .env("BAML_TELEMETRY", level)
                .env("BAML_GC_STRESS", stress)
                .env("BAML_HOME", home.path());
            let plain = run_probe(&mut command, Duration::from_secs(120)).unwrap();
            assert!(
                plain.status.success(),
                "{}",
                String::from_utf8_lossy(&plain.stderr)
            );
            assert_eq!(
                plain.stdout,
                if string_result {
                    b"\"hello\\n\\\"world\\\"\"\n".as_slice()
                } else {
                    b"777\n".as_slice()
                }
            );
            let actual = run_probe(
                command.args(["--jobs", "3", "--warmup", "2"]),
                Duration::from_secs(120),
            )
            .unwrap();
            assert!(
                actual.status.success(),
                "{}",
                String::from_utf8_lossy(&actual.stderr)
            );
            let report: serde_json::Value = serde_json::from_slice(&actual.stdout).unwrap();
            assert_eq!(report["result"], "93");
            assert_eq!(report["jobs"], 3);
            assert!(report["elapsed_ns"].as_u64().unwrap() > 0);
        }
    }
    // A changing checksum must fail: recreating setup state before each run
    // would incorrectly hide this mutation and make these checks succeed.
    let changing = input.replace(
        if string_result {
            "(state.payload.length() + state.stamp + sum % 7).to_string()"
        } else {
            "state.payload.length() + state.stamp + sum % 7"
        },
        if string_result {
            "state.stamp += 1; state.stamp.to_string()"
        } else {
            "state.stamp += 1; state.stamp"
        },
    );
    std::fs::write(source.join("baml_src/main.baml"), changing).unwrap();
    pack_result_options(
        &build,
        workspace,
        &source,
        &executable,
        ["main"],
        true,
        true,
    )
    .unwrap();
    for (warmup, message) in [
        ("1", "timed result mismatch"),
        ("2", "warmup result mismatch"),
    ] {
        let home = tempfile::tempdir().unwrap();
        let actual = run_probe(
            Command::new(&executable)
                .args(["--jobs", "2", "--warmup", warmup])
                .env("BAML_TELEMETRY", "off")
                .env("BAML_HOME", home.path()),
            Duration::from_secs(120),
        )
        .unwrap();
        assert!(!actual.status.success());
        assert!(
            String::from_utf8_lossy(&actual.stderr).contains(message),
            "{}",
            String::from_utf8_lossy(&actual.stderr)
        );
    }
}
