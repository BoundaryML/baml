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
};

/// Run `baml-cli` in `dir` with an isolated cache directory (so cold/warm state
/// is under the test's control) and the passive skill/update checks disabled.
/// `extra_env` layers on top of the shared spawn matrix (e.g. an opt-out knob).
fn run_list(
    cli: &Path,
    dir: &Path,
    cache_dir: &Path,
    args: &[&str],
    extra_env: &[(&str, &str)],
) -> Output {
    let home = dir.join(".baml-home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), "[update]\nauto_check = false\n").unwrap();
    let mut cmd = Command::new(cli);
    cmd.args(args);
    cmd.current_dir(dir);
    cmd.env("BAML_CLI_ALLOW_DIRECT", "1");
    // Pin the human output preset: under a coding agent the inherited
    // CLAUDECODE/AI_AGENT/… environment flips `--output-preset auto` to
    // `agent`, which disables the progress lines some assertions read.
    cmd.env("BAML_OUTPUT_PRESET", "human");
    cmd.env("BAML_HOME", &home);
    cmd.env("BAML_CACHE_DIR", cache_dir);
    cmd.env("BAML_CACHE_DEBUG", "1");
    for (key, value) in extra_env {
        cmd.env(key, value);
    }
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

const SERVED_FROM_CACHE: &str = "served `test --list` from discovery cache";

/// Cold run (fresh cache) then warm run (cache populated) of `baml test --list
/// <extra…>`. Asserts exit code AND stdout are identical, and that only the warm
/// run served from the discovery cache (proving it skipped engine boot). Works
/// for both matching filters (exit 0) and no-match filters (exit 5), so the
/// no-tests-selected edge is covered too.
fn assert_cold_equals_warm(cli: &Path, extra: &[&str]) {
    let tmp = tempfile::tempdir().unwrap();
    create_test_project(tmp.path());
    let cache_dir = tmp.path().join(".discovery-cache");

    let mut args = vec!["test", "--list", "--from", "."];
    args.extend_from_slice(extra);

    // The honest filtered path must not populate an unfiltered cache by
    // expanding profile-excluded lazy testsets. Force honest discovery, then
    // independently populate the cache with an explicitly unfiltered list and
    // prove that applying `extra` to that cache is output-identical.
    let cold = run_list(
        cli,
        tmp.path(),
        &cache_dir,
        &args,
        &[("BAML_NO_DISCOVERY_CACHE", "1")],
    );
    let _populate = run_list(
        cli,
        tmp.path(),
        &cache_dir,
        &["test", "--list", "--from", "."],
        &[],
    );
    let warm = run_list(cli, tmp.path(), &cache_dir, &args, &[]);

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

    let cold_err = String::from_utf8_lossy(&cold.stderr);
    let warm_err = String::from_utf8_lossy(&warm.stderr);
    assert!(
        !cold_err.contains(SERVED_FROM_CACHE),
        "the cold run must do honest discovery, not serve from cache; stderr:\n{cold_err}",
    );
    assert!(
        warm_err.contains(SERVED_FROM_CACHE),
        "the warm run must serve `--list {extra:?}` from the discovery cache (engine boot \
         skipped); stderr:\n{warm_err}",
    );
}

/// Unfiltered `--list`: the corpus measurement path and the common case. Also
/// sanity-checks that the fixture lists a non-empty set.
#[test]
fn list_unfiltered_cold_equals_warm() {
    let cli = common::baml_cli();
    assert_cold_equals_warm(&cli, &[]);

    // Independent sanity: the unfiltered list is non-empty and exit 0.
    let tmp = tempfile::tempdir().unwrap();
    create_test_project(tmp.path());
    let cache_dir = tmp.path().join(".discovery-cache");
    let out = run_list(
        &cli,
        tmp.path(),
        &cache_dir,
        &["test", "--list", "--from", "."],
        &[],
    );
    assert!(out.status.success(), "unfiltered `--list` should exit 0");
    assert!(
        stdout_of(&out).contains("root::suite::nested::deep"),
        "unfiltered list should render the nested leaf, got:\n{}",
        stdout_of(&out),
    );
}

/// A filter matrix. Each variant re-runs cold (BAML filter) vs warm (Rust
/// filter) from a fresh cache, so byte-identity across the matrix proves the two
/// filter implementations select the same leaves (including a no-match case).
#[test]
fn list_filtered_cold_equals_warm_across_matrix() {
    let cli = common::baml_cli();
    for extra in [
        vec!["-i", "root::suite::*"],
        vec!["-i", "root::suite::one"],
        vec!["-i", "root::suite::nested::deep"],
        vec!["-i", "root::top_alpha"],
        vec!["-i", "*::solo"],
        vec!["-i", "root::suite::*"],
        vec!["-x", "root::suite::*"],
        vec!["-i", "root::suite::*", "-x", "root::suite::two"],
        vec!["-i", "totally-bogus-selector-xyz"], // no match (exit 5, empty stdout)
    ] {
        assert_cold_equals_warm(&cli, &extra);
    }
}

/// `BAML_NO_DISCOVERY_CACHE=1` must be output-neutral (it only forces the honest
/// path): the knobbed run's stdout matches the honest cold run, and it never
/// serves from the discovery cache even when the cache is warm.
#[test]
fn no_discovery_cache_knob_is_output_neutral() {
    let cli = common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();
    create_test_project(tmp.path());
    let cache_dir = tmp.path().join(".discovery-cache");
    let args = ["test", "--list", "--from", "."];

    // Warm the discovery cache.
    let cold = run_list(&cli, tmp.path(), &cache_dir, &args, &[]);
    assert!(cold.status.success());
    let warm = run_list(&cli, tmp.path(), &cache_dir, &args, &[]);
    assert!(warm.status.success());
    assert!(String::from_utf8_lossy(&warm.stderr).contains(SERVED_FROM_CACHE));

    // With the knob set, the warm cache is ignored: honest discovery runs and
    // produces the same stdout.
    let knobbed = run_list(
        &cli,
        tmp.path(),
        &cache_dir,
        &args,
        &[("BAML_NO_DISCOVERY_CACHE", "1")],
    );
    assert!(knobbed.status.success());
    assert_eq!(
        stdout_of(&knobbed),
        stdout_of(&warm),
        "BAML_NO_DISCOVERY_CACHE must not change `--list` stdout",
    );
    assert!(
        !String::from_utf8_lossy(&knobbed.stderr).contains(SERVED_FROM_CACHE),
        "BAML_NO_DISCOVERY_CACHE=1 must force the honest path, not serve from cache",
    );
}
