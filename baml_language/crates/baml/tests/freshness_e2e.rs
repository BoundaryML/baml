//! End-to-end tests for the wrapper's toolchain freshness warning: run the real `baml`
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
        home.write_state();
        home.write_manifest_cache("0.11.0");
        home
    }

    /// Write `state.toml` with the canary channel active at 0.11.0.
    fn write_state(&self) {
        fs::write(
            self.root.join("state.toml"),
            "[channels.canary]\nactive_version = \"0.11.0\"\nresolved_at = \"x\"\nmanifest_path = \"y\"\n",
        )
        .unwrap();
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

    fn run(&self) -> Output {
        let cwd = tempfile::tempdir().unwrap();
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
