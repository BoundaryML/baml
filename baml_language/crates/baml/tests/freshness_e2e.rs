//! End-to-end tests for the wrapper's freshness warnings: run the real `baml`
//! binary against a temp `BAML_HOME` and assert on stderr.
//!
//! Unix-only: the tests install a fake shell-script `baml-cli` for
//! `pass_through` to exec.

#![cfg(unix)]

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const TOOLCHAIN_WARNING: &str = "warning: Your version of baml for toolchain: canary is outdated. Update it with baml toolchain update.";
const SKILL_OUTDATED_WARNING: &str =
    "warning: Your baml skill is outdated, use baml agent install to upgrade it.";
const SKILL_MISSING_WARNING: &str =
    "warning: No baml skill is installed, set it up with baml agent install.";

/// A project directory containing installed baml agent skills.
fn project_with_skills() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".agents/skills/baml-core")).unwrap();
    dir
}

/// A temp `BAML_HOME` seeded with an installed fake toolchain (version
/// `0.11.0`, tracked via the `canary` channel) whose `baml-cli` prints
/// `cli ran` and exits 0.
struct TestHome {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl TestHome {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();

        let bin = root.join("toolchains/0.11.0/bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(root.join("toolchains/0.11.0/VERSION"), "0.11.0\n").unwrap();
        let cli = bin.join("baml-cli");
        fs::write(&cli, "#!/bin/sh\necho \"cli ran\"\n").unwrap();
        #[allow(clippy::permissions_set_readonly_false)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&cli, fs::Permissions::from_mode(0o755)).unwrap();
        }

        fs::write(
            root.join("config.toml"),
            "[default]\nselector = \"canary\"\n\n[update]\nauto_check = false\n",
        )
        .unwrap();
        let home = Self { _dir: dir, root };
        home.write_state(None);
        home.write_manifest_cache("0.11.0");
        home
    }

    /// Write `state.toml` with the canary channel active at 0.11.0 and an
    /// optional `[skills]` provenance section.
    fn write_state(&self, installed_skill_commit: Option<&str>) {
        let mut text = String::from(
            "[channels.canary]\nactive_version = \"0.11.0\"\nresolved_at = \"x\"\nmanifest_path = \"y\"\n",
        );
        if let Some(commit) = installed_skill_commit {
            text.push_str("\n[skills]\ninstalled_commit = \"");
            text.push_str(commit);
            text.push_str("\"\ninstalled_at = \"x\"\n");
        }
        fs::write(self.root.join("state.toml"), text).unwrap();
    }

    fn write_manifest_cache(&self, version: &str) {
        let dir = self.root.join("manifest-cache/prod");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("canary.json"),
            format!(
                r#"{{"schema":1,"version":"{version}","channel":"canary","released_at":"x","artifacts":{{}}}}"#
            ),
        )
        .unwrap();
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
        fs::write(
            self.root.join("config.toml"),
            "[default]\nselector = \"canary\"\n",
        )
        .unwrap();
    }

    /// Run `baml hello` with this home, from `cwd`. `$HOME` is pointed at the
    /// cwd's parent so the project-skills walk stays inside the temp tree.
    fn run_from(&self, cwd: &Path, extra_env: &[(&str, &str)]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_baml"));
        command
            .arg("hello")
            .current_dir(cwd)
            .env("BAML_HOME", &self.root)
            .env("HOME", cwd.parent().unwrap_or(cwd))
            .env_remove("BAML_VERSION");
        for (key, value) in extra_env {
            command.env(key, value);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "wrapper failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("cli ran"),
            "fake baml-cli did not run"
        );
        output
    }

    /// Run from a project that has skills installed, so skill warnings don't
    /// bleed into tests that are about something else.
    fn run(&self) -> Output {
        let cwd = project_with_skills();
        self.run_from(cwd.path(), &[])
    }
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn warns_when_cached_manifest_is_newer_than_active_toolchain() {
    let home = TestHome::new();
    home.write_manifest_cache("0.12.0");
    let stderr = stderr_of(&home.run());
    assert!(stderr.contains(TOOLCHAIN_WARNING), "{stderr}");
}

#[test]
fn silent_when_toolchain_matches_cached_manifest() {
    let home = TestHome::new();
    let stderr = stderr_of(&home.run());
    assert!(!stderr.contains("outdated"), "{stderr}");
}

#[test]
fn warns_when_skill_provenance_is_behind_cached_latest() {
    let home = TestHome::new();
    home.write_state(Some("aaa"));
    home.write_skill_cache("bbb");
    let stderr = stderr_of(&home.run());
    assert!(stderr.contains(SKILL_OUTDATED_WARNING), "{stderr}");
}

#[test]
fn silent_when_skill_provenance_matches_cached_latest() {
    let home = TestHome::new();
    home.write_state(Some("aaa"));
    home.write_skill_cache("aaa");
    let stderr = stderr_of(&home.run());
    assert!(!stderr.contains("skill"), "{stderr}");
}

#[test]
fn missing_project_skills_prompt_install_on_every_command() {
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
fn project_skills_without_provenance_prompt_upgrade() {
    let home = TestHome::new();
    home.write_skill_cache("bbb");
    let project = project_with_skills();
    let stderr = stderr_of(&home.run_from(project.path(), &[]));
    assert!(stderr.contains(SKILL_OUTDATED_WARNING), "{stderr}");
    assert!(!stderr.contains(SKILL_MISSING_WARNING), "{stderr}");
}

#[test]
fn auto_check_failure_is_quiet_and_throttled_by_marker() {
    let home = TestHome::new();
    home.enable_auto_check();
    // Port 1 refuses connections immediately; the skill cache is absent so a
    // refresh attempt is due. The manifest cache is fresh, so no toolchain
    // fetch happens.
    let env = [("BAML_AGENT_SKILLS_COMMITS_URL", "http://127.0.0.1:1/")];
    let cwd = project_with_skills();

    let stderr = stderr_of(&home.run_from(cwd.path(), &env));
    assert!(!stderr.contains("outdated"), "{stderr}");
    let marker = home
        .root
        .join("manifest-cache/skills/latest-commit.json.last-check");
    assert!(marker.exists(), "attempt marker was not written");
    let marker_mtime = marker.metadata().unwrap().modified().unwrap();

    // Second run inside the TTL window: the marker throttles the retry.
    let stderr = stderr_of(&home.run_from(cwd.path(), &env));
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
    let cwd = project_with_skills();
    home.run_from(cwd.path(), &env);
    assert!(
        !home
            .root
            .join("manifest-cache/skills/latest-commit.json.last-check")
            .exists(),
        "auto_check=false must not attempt a refresh"
    );
}
