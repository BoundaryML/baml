mod common;

use std::{path::Path, process::Command};

fn create_project(dir: &Path) {
    std::fs::write(
        dir.join("baml.toml"),
        r#"[package]
name = "profile-e2e"

[test]
default = "regular"

[test.profiles.regular]
args = ["-x", "::integration::"]

[test.profiles.integration]
args = ["-i", "::integration::"]

[test.profiles.unit]
args = ["-i", "::unit::"]
"#,
    )
    .unwrap();
    let src = dir.join("baml_src/ns_orders");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("tests.baml"),
        r##"
testset "unit" {
  test "parses_order" { assert.is_true(true) }
}

testset "integration" {
  test "hello_test" { assert.is_true(true) }
  test "creates_order" { assert.is_true(true) }
}

client TestClient = openai.ResponsesClient.new(model = "gpt-4o-mini");

function Summarize(input: string) -> string {
  client: TestClient
  prompt: `${input}`
}

test BasicTest {
  functions [Summarize]
  args { input "hello" }
}
"##,
    )
    .unwrap();
}

fn run(dir: &Path, args: &[&str]) -> std::process::Output {
    run_with_env(dir, args, None)
}

fn run_with_env(dir: &Path, args: &[&str], env: Option<(&str, &str)>) -> std::process::Output {
    let home = dir.join(".baml-home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), "[update]\nauto_check = false\n").unwrap();
    let mut command = Command::new(common::baml_cli());
    command
        .args(args)
        .current_dir(dir)
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        // Pin the human preset so inherited agent env (CLAUDECODE/AI_AGENT/…)
        // cannot flip `--output-preset auto` to `agent` and hide progress lines.
        .env("BAML_OUTPUT_PRESET", "human")
        .env("BAML_HOME", home)
        .env("BAML_CACHE_DIR", dir.join(".baml-cache"));
    if let Some((name, value)) = env {
        command.env(name, value);
    }
    command.output().unwrap()
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn default_named_and_no_profile_select_expected_canonical_ids() {
    let tmp = tempfile::tempdir().unwrap();
    create_project(tmp.path());

    let regular = run(tmp.path(), &["test", "--list"]);
    assert!(
        regular.status.success(),
        "{}",
        String::from_utf8_lossy(&regular.stderr)
    );
    assert!(stdout(&regular).contains("root.orders::unit::parses_order"));
    assert!(stdout(&regular).contains("root.orders.Summarize::BasicTest"));
    assert!(!stdout(&regular).contains("integration"));

    let integration = run(tmp.path(), &["test", "--list", "--profile", "integration"]);
    assert!(
        integration.status.success(),
        "{}",
        String::from_utf8_lossy(&integration.stderr)
    );
    assert!(stdout(&integration).contains("root.orders::integration::hello_test"));
    assert!(stdout(&integration).contains("root.orders::integration::creates_order"));
    assert!(!stdout(&integration).contains("unit"));

    let all = run(tmp.path(), &["test", "--list", "--no-profile"]);
    assert!(
        all.status.success(),
        "{}",
        String::from_utf8_lossy(&all.stderr)
    );
    assert!(stdout(&all).contains("root.orders::unit::parses_order"));
    assert!(stdout(&all).contains("root.orders::integration::hello_test"));
    assert!(stdout(&all).contains("root.orders.Summarize::BasicTest"));
}

#[test]
fn cli_include_narrows_profile_instead_of_oring_with_it() {
    let tmp = tempfile::tempdir().unwrap();
    create_project(tmp.path());
    let selected = run(
        tmp.path(),
        &["test", "--list", "--profile", "integration", "-i", "hello"],
    );
    assert!(
        selected.status.success(),
        "{}",
        String::from_utf8_lossy(&selected.stderr)
    );
    assert!(stdout(&selected).contains("root.orders::integration::hello_test"));
    assert!(!stdout(&selected).contains("creates_order"));
    assert!(!stdout(&selected).contains("unit"));
}

#[test]
fn bad_profiles_and_old_slash_selectors_are_actionable() {
    let tmp = tempfile::tempdir().unwrap();
    create_project(tmp.path());

    let missing = run(tmp.path(), &["test", "--profile", "missing"]);
    assert!(!missing.status.success());
    let error = String::from_utf8_lossy(&missing.stderr);
    assert!(error.contains("profile `missing`"), "{error}");
    assert!(
        error.contains("regular") && error.contains("integration"),
        "{error}"
    );

    let slash = run(
        tmp.path(),
        &["test", "-i", "integration::nested/hello_test"],
    );
    assert!(!slash.status.success());
    let error = String::from_utf8_lossy(&slash.stderr);
    assert!(error.contains("old `/` hierarchy separator"), "{error}");
    assert!(error.contains("integration::nested::hello_test"), "{error}");

    let rooted = run(tmp.path(), &["test", "-i", "rooted::nested/case"]);
    assert!(!rooted.status.success());
    let error = String::from_utf8_lossy(&rooted.stderr);
    assert!(error.contains("old `/` hierarchy separator"), "{error}");
    assert!(error.contains("rooted::nested::case"), "{error}");
}

#[test]
fn profile_exclusion_prunes_a_lazy_testset_before_expansion() {
    let tmp = tempfile::tempdir().unwrap();
    create_project(tmp.path());
    std::fs::write(
        tmp.path().join("baml_src/ns_orders/tests.baml"),
        r#"
testset "unit" {
  test "works" { assert.is_true(true) }
}

testset "integration" {
  throw ("integration testset expanded")
  test "unreachable" { assert.is_true(true) }
}
"#,
    )
    .unwrap();

    let regular = run(tmp.path(), &["test", "--list"]);
    assert!(
        regular.status.success(),
        "{}",
        String::from_utf8_lossy(&regular.stderr)
    );
    assert!(stdout(&regular).contains("root.orders::unit::works"));
    assert!(!String::from_utf8_lossy(&regular.stderr).contains("expanded"));

    let all = run(tmp.path(), &["test", "--list", "--no-profile"]);
    assert!(all.status.success());
    assert!(stdout(&all).contains("root.orders::integration::(failed to expand)"));

    for args in [
        vec!["test", "--no-profile"],
        vec!["test", "--profile", "integration"],
    ] {
        let executed = run(tmp.path(), &args);
        assert!(!executed.status.success());
        let combined = format!(
            "{}{}",
            stdout(&executed),
            String::from_utf8_lossy(&executed.stderr)
        );
        let sentinel = "root.orders::integration::(failed to expand)";
        assert!(combined.contains(&format!("FAIL {sentinel}")), "{combined}");
        assert!(
            !combined.contains(&format!("PASS {sentinel}")),
            "{combined}"
        );
        let expected_counts = if args.contains(&"--no-profile") {
            "1 passed, 1 failed, 2 total"
        } else {
            "0 passed, 1 failed, 1 total"
        };
        assert!(combined.contains(expected_counts), "{combined}");
        assert_eq!(combined.matches(&format!("FAIL {sentinel}")).count(), 1);
    }
}

#[test]
fn includes_layer_contradictions_and_broad_excludes_prune_lazy_collectors() {
    let tmp = tempfile::tempdir().unwrap();
    create_project(tmp.path());
    std::fs::write(
        tmp.path().join("baml_src/ns_orders/tests.baml"),
        r#"
testset "unit" {
  test "works" { assert.is_true(true) }
}

testset "integration" {
  throw ("integration collector must stay lazy")
  test "unreachable" { assert.is_true(true) }
}
"#,
    )
    .unwrap();

    let include_only = run(tmp.path(), &["test", "--list", "--profile", "unit"]);
    assert!(include_only.status.success());
    assert!(stdout(&include_only).contains("root.orders::unit::works"));
    assert!(!stdout(&include_only).contains("failed to expand"));

    let contradictory = run(
        tmp.path(),
        &[
            "test",
            "--list",
            "--profile",
            "integration",
            "-i",
            "root.orders::unit::*",
        ],
    );
    assert!(!contradictory.status.success());
    assert!(!stdout(&contradictory).contains("failed to expand"));

    for exclude in ["*", "root", "root*"] {
        let excluded = run(
            tmp.path(),
            &["test", "--list", "--no-profile", "-x", exclude],
        );
        assert!(!excluded.status.success());
        assert!(!stdout(&excluded).contains("failed to expand"));
    }
}

#[test]
fn double_colon_is_reserved_inside_declared_test_names() {
    let tmp = tempfile::tempdir().unwrap();
    create_project(tmp.path());
    std::fs::write(
        tmp.path().join("baml_src/ns_orders/tests.baml"),
        r#"test "bad::name" { assert.is_true(true) }"#,
    )
    .unwrap();

    let checked = run(tmp.path(), &["check"]);
    assert!(!checked.status.success());
    let check_error = String::from_utf8_lossy(&checked.stderr);
    assert!(
        check_error.contains("reserved separator `::`"),
        "{check_error}"
    );
    assert!(check_error.contains("testset"), "{check_error}");

    let output = run(tmp.path(), &["test", "--list", "--no-profile"]);
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("reserved separator `::`"), "{error}");
}

#[test]
fn nested_reserved_names_are_discovery_errors_not_sentinel_tests() {
    let tmp = tempfile::tempdir().unwrap();
    create_project(tmp.path());
    std::fs::write(
        tmp.path().join("baml_src/ns_orders/tests.baml"),
        r#"
testset "outer" {
  test "bad::name" { assert.is_true(true) }
}
"#,
    )
    .unwrap();

    let checked = run(tmp.path(), &["check"]);
    assert!(!checked.status.success());
    let check_error = String::from_utf8_lossy(&checked.stderr);
    assert!(
        check_error.contains("reserved separator `::`"),
        "{check_error}"
    );

    let output = run(tmp.path(), &["test", "--list", "--no-profile"]);
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("reserved separator `::`"), "{error}");
    assert!(!stdout(&output).contains("(failed to expand)"));

    let execution = run(tmp.path(), &["test", "--no-profile"]);
    assert!(!execution.status.success());
    let execution_error = String::from_utf8_lossy(&execution.stderr);
    assert!(
        execution_error.contains("reserved separator `::`"),
        "{execution_error}"
    );
    let combined = format!("{}{}", stdout(&execution), execution_error);
    assert!(!combined.contains("(testset error)"), "{combined}");
    assert!(!combined.contains("(failed to expand)"), "{combined}");
}

#[test]
fn console_leaf_ids_match_list_ids_for_passes_and_failures() {
    let tmp = tempfile::tempdir().unwrap();
    create_project(tmp.path());
    std::fs::write(
        tmp.path().join("baml_src/ns_orders/tests.baml"),
        r#"
testset "console" {
  test "passes" { assert.is_true(true) }
  test "fails" { assert.is_true(false) }
}
"#,
    )
    .unwrap();

    let listed = run(tmp.path(), &["test", "--list", "--no-profile"]);
    assert!(listed.status.success());
    let listed = stdout(&listed);
    let pass_id = "root.orders::console::passes";
    let fail_id = "root.orders::console::fails";
    assert!(listed.contains(pass_id), "{listed}");
    assert!(listed.contains(fail_id), "{listed}");

    let executed = run(tmp.path(), &["test", "--no-profile"]);
    assert!(!executed.status.success());
    let combined = format!(
        "{}{}",
        stdout(&executed),
        String::from_utf8_lossy(&executed.stderr)
    );
    assert!(combined.contains(&format!("PASS {pass_id}")), "{combined}");
    assert!(combined.contains(&format!("FAIL {fail_id}")), "{combined}");
    assert_eq!(combined.matches("PASS root.").count(), 1, "{combined}");
    assert_eq!(combined.matches("FAIL root.").count(), 1, "{combined}");
    assert!(
        combined.contains("1 passed, 1 failed, 2 total"),
        "{combined}"
    );
    assert!(
        combined.contains("AGGREGATE FAIL [outcome=fail]"),
        "{combined}"
    );
    assert!(!combined.contains("root::*"), "{combined}");
}

#[test]
fn failed_expansion_is_not_cached_and_literal_slash_selector_is_legal() {
    let tmp = tempfile::tempdir().unwrap();
    create_project(tmp.path());
    std::fs::write(
        tmp.path().join("baml_src/ns_orders/tests.baml"),
        r#"
testset "transient" {
  if (baml.env.get("PROFILE_EXPAND_OK") != "1") {
    throw ("transient expansion failure")
  }
  test "path/to/case" { assert.is_true(true) }
}
"#,
    )
    .unwrap();

    let cold_failure = run(tmp.path(), &["test", "--list", "--no-profile"]);
    assert!(cold_failure.status.success());
    assert!(stdout(&cold_failure).contains("(failed to expand)"));

    let retry = run_with_env(
        tmp.path(),
        &["test", "--list", "--no-profile", "-i", "*::path/to/case"],
        Some(("PROFILE_EXPAND_OK", "1")),
    );
    assert!(
        retry.status.success(),
        "{}",
        String::from_utf8_lossy(&retry.stderr)
    );
    assert!(stdout(&retry).contains("root.orders::transient::path/to/case"));
    assert!(!stdout(&retry).contains("(failed to expand)"));
}
