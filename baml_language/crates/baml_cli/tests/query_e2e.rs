//! End-to-end `baml query` (TASK/baml-query-scope.md §6): a real
//! `baml run` writes the profile store, then SQL reads it back with the
//! documented output contract and exit codes.

mod common;

use std::{
    path::Path,
    process::{Command, Output},
};

fn run_baml_cli(built: &Path, dir: &Path, args: &[&str]) -> Output {
    let home = dir.join(".baml-home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), "[update]\nauto_check = false\n").unwrap();
    let mut cmd = Command::new(built);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.current_dir(dir);
    cmd.env("BAML_CLI_ALLOW_DIRECT", "1");
    cmd.env("BAML_OUTPUT_PRESET", "human");
    cmd.env("BAML_HOME", &home);
    cmd.env_remove("BAML_LOG");
    cmd.env("BAML_CACHE_DIR", common::shared_cache_dir());
    cmd.output().expect("spawn baml-cli")
}

fn project(dir: &Path) {
    std::fs::create_dir_all(dir.join("baml_src")).unwrap();
    std::fs::write(
        dir.join("baml_src/main.baml"),
        r#"function helper(x: int) -> int {
  x * 2
}

function main() -> int {
  helper(21)
}
"#,
    )
    .unwrap();
}

#[test]
fn query_reads_back_a_run_with_the_documented_contract() {
    let cli = common::baml_cli();
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    project(dir);

    // `-v` surfaces the profiler's setup diagnostic on stderr: when the
    // store is missing, the failure message below carries the reason
    // instead of just the symptom.
    let run = run_baml_cli(&cli, dir, &["run", "main", "-v"]);
    assert!(
        run.status.success(),
        "baml run: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        dir.join(".baml/profiles-v1/streams").is_dir(),
        "the run writes the profile store; stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // Executions idiom, table output, complete outcome, exit 0.
    let query = run_baml_cli(
        &cli,
        dir,
        &[
            "query",
            "SELECT status, entry_fqn, total_calls FROM threads \
             WHERE parent_thread_id IS NULL",
        ],
    );
    let stdout = String::from_utf8_lossy(&query.stdout);
    let stderr = String::from_utf8_lossy(&query.stderr);
    assert_eq!(query.status.code(), Some(0), "stderr: {stderr}");
    assert!(stdout.contains("succeeded"), "rows on stdout: {stdout}");
    assert!(stdout.contains("user.main"), "entry fqn resolves: {stdout}");
    assert!(
        stderr.contains("result=complete"),
        "outcome on stderr: {stderr}"
    );

    // jsonl: one row object per line plus the terminal outcome frame.
    let query = run_baml_cli(
        &cli,
        dir,
        &[
            "query",
            "SELECT fqn FROM call_path_stats ORDER BY fqn",
            "--format",
            "jsonl",
        ],
    );
    assert_eq!(query.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&query.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3, "two rows + outcome frame: {stdout}");
    assert!(lines[0].contains("user.helper"));
    assert!(lines[1].contains("user.main"));
    assert!(
        lines[2].contains("\"queryOutcome\"") && lines[2].contains("\"resultState\":\"complete\""),
        "terminal frame: {}",
        lines[2]
    );

    // Unknown table: exit 2 with a did-you-mean remedy.
    let query = run_baml_cli(&cli, dir, &["query", "SELECT * FROM thread"]);
    assert_eq!(query.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&query.stderr).contains("did you mean `threads`?"),
        "remedy: {}",
        String::from_utf8_lossy(&query.stderr)
    );

    // DML is rejected as invalid SQL.
    let query = run_baml_cli(&cli, dir, &["query", "DROP TABLE calls"]);
    assert_eq!(query.status.code(), Some(2));

    // --schema renders the profile.
    let query = run_baml_cli(
        &cli,
        dir,
        &["query", "--schema", "--table", "calls", "--format", "json"],
    );
    assert_eq!(query.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&query.stdout);
    assert!(stdout.contains("\"catalogVersion\":\"v1\""));
    assert!(stdout.contains("\"name\":\"calls_v1\""));

    // Budget exhaustion is terminal and typed (exit 3).
    let query = run_baml_cli(
        &cli,
        dir,
        &["query", "SELECT metric FROM health", "--max-rows", "1"],
    );
    assert_eq!(
        query.status.code(),
        Some(3),
        "stderr: {}",
        String::from_utf8_lossy(&query.stderr)
    );
}

#[test]
fn query_without_a_store_fails_with_a_remedy() {
    let cli = common::baml_cli();
    let temp = tempfile::tempdir().unwrap();
    project(temp.path());
    let query = run_baml_cli(&cli, temp.path(), &["query", "SHOW TABLES"]);
    assert_eq!(query.status.code(), Some(5));
    assert!(
        String::from_utf8_lossy(&query.stderr).contains("run a BAML program first"),
        "remedy: {}",
        String::from_utf8_lossy(&query.stderr)
    );
}
