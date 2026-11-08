//! Independent project: global-initializer admission in the shared probes must
//! not hide regressions in typed code generation.
use super::*;
const SOURCE: &str = include_str!("type_dispatch.baml");

#[allow(clippy::disallowed_methods, reason = "test harness configuration")]
pub(super) fn run() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let build = std::env::var_os("BAML_RUST_DIFF_BUILD_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_TARGET_TMPDIR")).join("rust-differential"));
    let source = build.join("type-dispatch-source");
    std::fs::create_dir_all(source.join("baml_src")).unwrap();
    std::fs::write(
        source.join("baml.toml"),
        "[package]\nname = \"typed-dispatch\"\n[pack.env_var_names]\nBAML_TELEMETRY = \"BAML_TELEMETRY\"\n",
    )
    .unwrap();
    std::fs::write(source.join("baml_src/main.baml"), SOURCE).unwrap();
    let vm = build.join("type-dispatch-vm");
    let rt = build.join("type-dispatch-rt");
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
