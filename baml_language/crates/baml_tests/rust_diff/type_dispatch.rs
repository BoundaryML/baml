//! Independent project: global-initializer admission in the shared probes must
//! not hide regressions in typed code generation.
use super::*;
const SOURCE: &str = include_str!("type_dispatch.baml");

#[allow(clippy::disallowed_methods, reason = "test harness configuration")]
pub(super) fn run() {
    run_source("type-dispatch", SOURCE);
}

pub(super) fn run_source(name: &str, input: &str) {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let build = std::env::var_os("BAML_RUST_DIFF_BUILD_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_TARGET_TMPDIR")).join("rust-differential"));
    let source = build.join(format!("{name}-source"));
    std::fs::create_dir_all(source.join("baml_src")).unwrap();
    std::fs::write(
        source.join("baml.toml"),
        "[package]\nname = \"typed-dispatch\"\n[pack.env_var_names]\nBAML_TELEMETRY = \"BAML_TELEMETRY\"\n",
    )
    .unwrap();
    std::fs::write(source.join("baml_src/main.baml"), input).unwrap();
    let vm = build.join(format!("{name}-vm"));
    let rt = build.join(format!("{name}-rt"));
    pack_result(&build, workspace, &source, &vm, ["main"], false).unwrap();
    pack_result(&build, workspace, &source, &rt, ["main"], true).unwrap();
    let home = tempfile::tempdir().unwrap();
    let oracle = run_probe(
        Command::new(&vm)
            .arg("main")
            .env("BAML_COMPILED_BODIES", "0")
            .env("BAML_TELEMETRY", "off")
            .env("BAML_HOME", home.path()),
        Duration::from_secs(120),
    )
    .unwrap();
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    for stress in ["0", "1"] {
        let actual = run_probe(
            Command::new(&rt)
                .env("BAML_GC_STRESS", stress)
                .env("BAML_TELEMETRY", "off")
                .env("BAML_HOME", home.path()),
            Duration::from_secs(120),
        )
        .unwrap();
        assert_eq!(
            actual.status,
            oracle.status,
            "{}",
            String::from_utf8_lossy(&actual.stderr)
        );
        assert_eq!(actual.stdout, oracle.stdout);
        assert_eq!(actual.stderr, oracle.stderr);
    }
}

pub(super) fn compare_json_telemetry() {
    let build = Path::new(env!("CARGO_TARGET_TMPDIR")).join("rust-differential");
    for level in ["low", "medium", "high"] {
        let run = |is_rt: bool, stress: bool| {
            let home = tempfile::tempdir_in(&build).unwrap();
            let mut command = Command::new(build.join(if is_rt {
                "json-runtime-rt"
            } else {
                "json-runtime-vm"
            }));
            if !is_rt {
                command.arg("main");
            }
            command
                .env("BAML_HOME", home.path())
                .env("BAML_TELEMETRY", level)
                .env("BAML_COMPILED_BODIES", "0")
                .env("BAML_GC_STRESS", if stress { "1" } else { "0" });
            let output = run_probe(&mut command, Duration::from_secs(120)).unwrap();
            let trace = telemetry::read(home.path());
            assert!(!trace.is_empty(), "{level}: missing telemetry");
            (
                output,
                known_telemetry_divergences::without_gc(&trace, "rt"),
            )
        };
        let (expected, trace) = run(false, false);
        assert!(
            expected.status.success(),
            "{}",
            String::from_utf8_lossy(&expected.stderr)
        );
        for stress in [false, true] {
            let (actual, observed) = run(true, stress);
            assert_eq!(
                actual.status.code(),
                expected.status.code(),
                "{level}/{stress}: {}",
                String::from_utf8_lossy(&actual.stderr)
            );
            assert_eq!(actual.stdout, expected.stdout);
            assert_eq!(actual.stderr, expected.stderr);
            if observed != trace {
                std::fs::write(build.join("json-telemetry-expected.txt"), &trace).unwrap();
                std::fs::write(build.join("json-telemetry-actual.txt"), &observed).unwrap();
                panic!(
                    "{level}/{stress}: {}",
                    diff(&trace, &observed, "json telemetry")
                );
            }
        }
    }
}
