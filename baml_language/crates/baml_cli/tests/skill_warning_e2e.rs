//! End-to-end tests for the toolchain's passive agent-skill warning: run the
//! real `baml-cli` binary against a temp `BAML_HOME` and assert on stderr.
//!
//! The warning fires only on the whitelisted authoring commands (init, run,
//! generate, pack) and prints before the subcommand dispatches, so the tests
//! use `generate` in an empty directory — the command itself fails ("no .baml
//! files"), which is irrelevant to what's being asserted.

use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const SKILL_OUTDATED_WARNING: &str =
    "your baml skill is outdated; use `baml agent install` to upgrade it";
const SKILL_MISSING_WARNING: &str =
    "no baml skill is installed; set it up with `baml agent install`";

/// A project directory containing installed baml agent skills.
fn project_with_skills() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let skill_dir = dir.path().join(".agents/skills/baml-core");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(skill_dir.join("SKILL.md"), "---\nname: baml-core\n---\n").unwrap();
    dir
}

/// A temp `BAML_HOME`. Auto-check is disabled by default so tests that aren't
/// about the background refresh never touch the network.
struct TestHome {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl TestHome {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::write(root.join("config.toml"), "[update]\nauto_check = false\n").unwrap();
        Self { _dir: dir, root }
    }

    /// Write `state.toml` with an optional `[skills]` provenance section.
    fn write_state(&self, installed_skill_commit: Option<&str>) {
        let mut text = String::new();
        if let Some(commit) = installed_skill_commit {
            text.push_str("[skills]\ninstalled_commit = \"");
            text.push_str(commit);
            text.push_str("\"\ninstalled_at = \"x\"\n");
        }
        fs::write(self.root.join("state.toml"), text).unwrap();
    }

    fn write_skill_cache(&self, sha: &str) {
        let dir = self.root.join("manifest-cache/skills");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("latest-commit.json"),
            format!(r#"{{"sha":"{sha}"}}"#),
        )
        .unwrap();
    }

    fn enable_auto_check(&self) {
        fs::write(self.root.join("config.toml"), "").unwrap();
    }

    /// Run `baml-cli generate` (a warning-whitelisted command) from `cwd`.
    /// `$HOME` is pointed at the cwd's parent so the project-skills walk
    /// stays inside the temp tree.
    fn run_from(&self, cwd: &Path, extra_env: &[(&str, &str)]) -> Output {
        self.run_args_from(&["generate"], cwd, extra_env)
    }

    fn run_args_from(&self, args: &[&str], cwd: &Path, extra_env: &[(&str, &str)]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_baml-cli"));
        command
            .args(args)
            .current_dir(cwd)
            .env("BAML_HOME", &self.root)
            .env("HOME", cwd.parent().unwrap_or(cwd))
            .env("BAML_WRAPPER_EXEC", "1")
            .env("DO_NOT_TRACK", "1");
        for (key, value) in extra_env {
            command.env(key, value);
        }
        command.output().unwrap()
    }
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn non_whitelisted_commands_never_warn() {
    let home = TestHome::new();
    // Same skill-less setup that makes `generate` warn — `check` is not in
    // the init/run/generate/pack whitelist, so it stays quiet.
    let empty = tempfile::tempdir().unwrap();
    let stderr = stderr_of(&home.run_args_from(&["check"], empty.path(), &[]));
    assert!(!stderr.contains(SKILL_MISSING_WARNING), "{stderr}");
    let stderr = stderr_of(&home.run_args_from(&["fmt"], empty.path(), &[]));
    assert!(!stderr.contains(SKILL_MISSING_WARNING), "{stderr}");
}

#[test]
fn missing_project_skills_prompt_install_on_whitelisted_commands() {
    let home = TestHome::new();

    // No skills in the project: prompt to install, even with no caches at all.
    let empty = tempfile::tempdir().unwrap();
    let stderr = stderr_of(&home.run_from(empty.path(), &[]));
    assert!(stderr.contains(SKILL_MISSING_WARNING), "{stderr}");

    // Still the install prompt (not the upgrade one) when a latest-commit
    // cache and even matching global provenance exist.
    home.write_state(Some("bbb"));
    home.write_skill_cache("bbb");
    let stderr = stderr_of(&home.run_from(empty.path(), &[]));
    assert!(stderr.contains(SKILL_MISSING_WARNING), "{stderr}");
    assert!(!stderr.contains(SKILL_OUTDATED_WARNING), "{stderr}");
}

#[test]
fn empty_skill_directory_still_prompts_install() {
    let home = TestHome::new();

    // A leftover baml-* directory without a SKILL.md is not an installation.
    let project = tempfile::tempdir().unwrap();
    fs::create_dir_all(project.path().join(".agents/skills/baml-core")).unwrap();

    let stderr = stderr_of(&home.run_from(project.path(), &[]));
    assert!(stderr.contains(SKILL_MISSING_WARNING), "{stderr}");
    assert!(!stderr.contains(SKILL_OUTDATED_WARNING), "{stderr}");
}

#[test]
fn archived_old_skill_does_not_count_as_installed() {
    let home = TestHome::new();

    // A skill that only exists in the baml-old_skills/ archive (nested one
    // level deeper than the <skills>/<name>/SKILL.md layout) is not an
    // installation and must not suppress the missing-skill prompt.
    let project = tempfile::tempdir().unwrap();
    let archived = project
        .path()
        .join(".claude/skills/baml-old_skills/baml-core");
    fs::create_dir_all(&archived).unwrap();
    fs::write(archived.join("SKILL.md"), "---\nname: baml-core\n---\n").unwrap();

    let stderr = stderr_of(&home.run_from(project.path(), &[]));
    assert!(stderr.contains(SKILL_MISSING_WARNING), "{stderr}");
}

#[test]
fn warns_when_skill_provenance_is_behind_cached_latest() {
    let home = TestHome::new();
    home.write_state(Some("aaa"));
    home.write_skill_cache("bbb");
    let project = project_with_skills();
    let stderr = stderr_of(&home.run_from(project.path(), &[]));
    assert!(stderr.contains(SKILL_OUTDATED_WARNING), "{stderr}");
}

#[test]
fn silent_when_skill_provenance_matches_cached_latest() {
    let home = TestHome::new();
    home.write_state(Some("aaa"));
    home.write_skill_cache("aaa");
    let project = project_with_skills();
    let stderr = stderr_of(&home.run_from(project.path(), &[]));
    assert!(!stderr.contains("skill"), "{stderr}");
}

#[test]
fn project_skills_without_provenance_prompt_upgrade() {
    let home = TestHome::new();
    home.write_skill_cache("bbb");
    let project = project_with_skills();
    let stderr = stderr_of(&home.run_from(project.path(), &[]));
    assert!(stderr.contains(SKILL_OUTDATED_WARNING), "{stderr}");
    assert!(!stderr.contains(SKILL_MISSING_WARNING), "{stderr}");
}

#[test]
fn agent_command_never_nags() {
    let home = TestHome::new();
    // A skill-less project would normally prompt to install; `baml agent …`
    // is the remedy itself, so it must stay quiet. (The install itself fails
    // on the bogus --from source, which is fine: the warning would have
    // printed before dispatch.)
    let empty = tempfile::tempdir().unwrap();
    let stderr = stderr_of(&home.run_args_from(
        &["agent", "install", "--from", "/nonexistent-skill-source"],
        empty.path(),
        &[],
    ));
    assert!(!stderr.contains(SKILL_MISSING_WARNING), "{stderr}");
}

/// Serve GitHub-commit-shaped JSON (`{"sha": ...}`) for every request.
/// Returns the server's base URL.
fn spawn_stub_commits_server(sha: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let body = format!(r#"{{"sha":"{sha}"}}"#);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            );
        }
    });
    base
}

#[test]
fn background_refresh_persists_cache_and_next_command_warns() {
    let home = TestHome::new();
    home.enable_auto_check();
    home.write_state(Some("aaa"));
    // No skill cache: the first command has nothing to compare against, so it
    // stays silent up front while the background refresh (stubbed to return a
    // newer commit) fills the cache.
    let server = spawn_stub_commits_server("bbb");
    let env = [("BAML_AGENT_SKILLS_COMMITS_URL", server.as_str())];
    let project = project_with_skills();

    let stderr = stderr_of(&home.run_from(project.path(), &env));
    assert!(!stderr.contains(SKILL_OUTDATED_WARNING), "{stderr}");
    assert_eq!(
        fs::read_to_string(home.root.join("manifest-cache/skills/latest-commit.json")).unwrap(),
        r#"{"sha":"bbb"}"#,
        "background refresh did not persist the cache"
    );

    // Next command: the fresh cache justifies the warning, printed once.
    let stderr = stderr_of(&home.run_from(project.path(), &env));
    assert_eq!(
        stderr.matches(SKILL_OUTDATED_WARNING).count(),
        1,
        "{stderr}"
    );
}

#[test]
fn auto_check_failure_is_quiet_and_throttled_by_marker() {
    let home = TestHome::new();
    home.enable_auto_check();
    // Port 1 refuses connections immediately; the skill cache is absent so a
    // refresh attempt is due.
    let env = [("BAML_AGENT_SKILLS_COMMITS_URL", "http://127.0.0.1:1/")];
    let project = project_with_skills();

    let stderr = stderr_of(&home.run_from(project.path(), &env));
    assert!(!stderr.contains("outdated"), "{stderr}");
    let marker = home
        .root
        .join("manifest-cache/skills/latest-commit.json.last-check");
    assert!(marker.exists(), "attempt marker was not written");
    let marker_mtime = marker.metadata().unwrap().modified().unwrap();

    // Second run inside the TTL window: the marker throttles the retry.
    let stderr = stderr_of(&home.run_from(project.path(), &env));
    assert!(!stderr.contains("outdated"), "{stderr}");
    assert_eq!(
        marker.metadata().unwrap().modified().unwrap(),
        marker_mtime,
        "marker was rewritten; retry was not throttled"
    );
}

#[test]
fn auto_check_optout_skips_refresh_entirely() {
    let home = TestHome::new();
    // auto_check = false (TestHome default config) and no skill cache: no
    // attempt marker may appear even though a refresh would be due.
    let env = [("BAML_AGENT_SKILLS_COMMITS_URL", "http://127.0.0.1:1/")];
    let project = project_with_skills();
    home.run_from(project.path(), &env);
    assert!(
        !home
            .root
            .join("manifest-cache/skills/latest-commit.json.last-check")
            .exists(),
        "auto_check=false must not attempt a refresh"
    );
}
