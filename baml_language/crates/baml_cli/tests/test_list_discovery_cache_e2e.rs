// End-to-end oracle for the `baml test --list` discovery cache (engine-boot
// floor). The invariant: `--list` stdout is byte-identical whether it is
// produced by an honest engine-boot discovery (cold) or served from the cached
// flattened test list (warm) — for the unfiltered list and every -i/-x variant.
//
// Because the cold run filters testset leaves in BAML (`testing.leaf_selected`)
// while the warm run filters them in Rust (`TestFilter`), a byte-identical
// cold==warm result across a filter matrix also proves those two filter
// implementations agree (design §6 "Rust-filter parity vs BAML select_names").

mod common;

use std::{
    path::Path,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

/// Run `baml-cli` in `dir` with `home` as its `BAML_HOME` (the build cache
/// lives under it, so cold/warm state is under the test's control) and the
/// passive skill/update checks disabled.
fn run_list(cli: &Path, dir: &Path, home: &Path, args: &[&str]) -> Output {
    std::fs::create_dir_all(home).unwrap();
    std::fs::write(home.join("config.toml"), "[update]\nauto_check = false\n").unwrap();
    let mut cmd = Command::new(cli);
    // Ignore the inherited CLAUDECODE/AI_AGENT/… environment: it flips
    // `--output-preset auto` to `agent`, which disables the progress lines some
    // assertions read.
    cmd.env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1");
    cmd.args(args);
    cmd.current_dir(dir);
    cmd.env("BAML_CLI_ALLOW_DIRECT", "1");
    cmd.env("BAML_HOME", home);
    cmd.output().expect("spawn baml-cli")
}

/// A project exercising the discovery paths `--list` renders: top-level tests
/// (`root::name`), a named testset with leaves, and a nested canonical id.
fn create_test_project(dir: &Path) {
    std::fs::write(
        dir.join("baml.toml"),
        "[package]\nname = \"discovery-cache-oracle\"\n",
    )
    .unwrap();
    let src = dir.join("baml_src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("main.baml"),
        r#"
test "top_alpha" {
  assert.is_true(true)
}

test "top_beta" {
  assert.is_true(true)
}

testset "suite" {
  test "one" { assert.is_true(true) }
  test "two" { assert.is_true(true) }

  testset "nested" {
    test "deep" { assert.is_true(true) }
  }
}

testset "other" {
  test "solo" { assert.is_true(true) }
}
"#,
    )
    .unwrap();
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Compare a cold run (a fresh home, so honest discovery) with a run against
/// an already-populated home (served from the cached discovery output).
/// Asserts exit code AND stdout are identical. Works for both matching filters
/// (exit 0) and no-match filters (exit 5), so the no-tests-selected edge is
/// covered too.
fn assert_honest_equals_warm(cli: &Path, dir: &Path, warm_home: &Path, extra: &[&str]) -> String {
    static COLD_HOMES: AtomicUsize = AtomicUsize::new(0);
    let mut args = vec!["test", "--list", "--from", "."];
    args.extend_from_slice(extra);

    // The cold side never sees the populated entry. The warm side applies
    // `extra` to that one canonical entry in Rust.
    let cold_home = dir.join(format!(
        ".cold-home-{}",
        COLD_HOMES.fetch_add(1, Ordering::Relaxed)
    ));
    let cold = run_list(cli, dir, &cold_home, &args);
    let warm = run_list(cli, dir, warm_home, &args);

    assert_eq!(
        cold.status.code(),
        warm.status.code(),
        "`--list {extra:?}` exit code diverged (cold {:?} vs warm {:?})\ncold stderr: {}\nwarm stderr: {}",
        cold.status.code(),
        warm.status.code(),
        String::from_utf8_lossy(&cold.stderr),
        String::from_utf8_lossy(&warm.stderr),
    );
    let cold_out = stdout_of(&cold);
    let warm_out = stdout_of(&warm);
    assert_eq!(
        cold_out, warm_out,
        "`--list {extra:?}` stdout diverged between cold (honest, BAML filter) and warm \
         (cached, Rust filter) runs\ncold:\n{cold_out}\nwarm:\n{warm_out}",
    );

    cold_out
}

fn populate_unfiltered_cache(cli: &Path, dir: &Path, home: &Path) -> String {
    let populate = run_list(cli, dir, home, &["test", "--list", "--from", "."]);
    assert!(
        populate.status.success(),
        "unfiltered cache population failed:\n{}",
        String::from_utf8_lossy(&populate.stderr)
    );
    assert!(
        home.join("build").join("cache").is_dir(),
        "population should have written the build cache under BAML_HOME"
    );
    stdout_of(&populate)
}

#[test]
fn list_unfiltered_cold_equals_warm() {
    let cli = common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();
    create_test_project(tmp.path());
    let warm_home = tmp.path().join(".warm-home");
    let populated_output = populate_unfiltered_cache(&cli, tmp.path(), &warm_home);
    let honest_output = assert_honest_equals_warm(&cli, tmp.path(), &warm_home, &[]);
    assert_eq!(
        populated_output, honest_output,
        "the initial cache-populating list must match cold and warm output"
    );
    assert!(
        populated_output.contains("root::suite::nested::deep"),
        "unfiltered list should render the nested leaf, got:\n{populated_output}",
    );
}

/// A filter matrix over one project and one unfiltered discovery-cache entry.
/// Each variant still performs an independent cold BAML-filter run and
/// compares it with the Rust-filtered cached result, including a no-match case.
#[test]
fn list_filtered_cold_equals_warm_across_matrix() {
    let cli = common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();
    create_test_project(tmp.path());
    let warm_home = tmp.path().join(".warm-home");
    let _ = populate_unfiltered_cache(&cli, tmp.path(), &warm_home);

    for extra in [
        vec!["-i", "root::suite::*"],
        vec!["-i", "root::suite::one"],
        vec!["-i", "root::suite::nested::deep"],
        vec!["-i", "root::top_alpha"],
        vec!["-i", "*::solo"],
        vec!["-x", "root::suite::*"],
        vec!["-i", "root::suite::*", "-x", "root::suite::two"],
        vec!["-i", "totally-bogus-selector-xyz"], // no match (exit 5, empty stdout)
    ] {
        let _ = assert_honest_equals_warm(&cli, tmp.path(), &warm_home, &extra);
    }
}
