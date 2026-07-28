//! End-to-end tests for wrapper output and delegation.
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

    fn write_cli(&self, script: &str) {
        let cli = self.root.join("toolchains/0.11.0/bin/baml-cli");
        fs::write(&cli, script).unwrap();
        #[allow(clippy::permissions_set_readonly_false)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&cli, fs::Permissions::from_mode(0o755)).unwrap();
        }
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
        let output = self.run_args_from(cwd, &["hello"], extra_env);
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

    fn run_args(&self, args: &[&str]) -> Output {
        let cwd = tempfile::tempdir().unwrap();
        self.run_args_from(cwd.path(), args, &[])
    }

    fn run_args_from(&self, cwd: &Path, args: &[&str], extra_env: &[(&str, &str)]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_baml"));
        command
            .args(args)
            .current_dir(cwd)
            .env("BAML_HOME", &self.root)
            .env("HOME", cwd.parent().unwrap_or(cwd))
            .env_remove("BAML_VERSION");
        for (key, value) in extra_env {
            command.env(key, value);
        }
        command.output().unwrap()
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

#[test]
fn root_help_merges_toolchain_metadata_with_wrapper_commands() {
    let home = TestHome::new();
    home.write_cli(
        r#"#!/bin/sh
if [ "$1" = "__baml-root-help-v1" ]; then
  [ "$BAML_WRAPPER_EXEC" = "1" ] || exit 8
  [ "$BAML_WRAPPER_RESOLVED_TOOLCHAIN" = "0.11.0" ] || exit 8
  printf '%s\n' '{"schema_version":"baml.root-help.v1","name":"baml","version":"0.11.0","about":"BAML CLI","usage":"baml [OPTIONS] <COMMAND>","commands":[{"syntax":"check","summary":"Check BAML"},{"syntax":"help","summary":"Print help"}],"options":[{"syntax":"-h, --help","summary":"Print help"}]}'
  exit 0
fi
exit 9
"#,
    );
    let output = home.run_args(&["--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "check",
        "Check BAML",
        "toolchain",
        "Manage installed BAML toolchains",
        "self-update",
        "Update the BAML wrapper",
    ] {
        assert!(stdout.contains(expected), "{stdout}");
    }
    assert_eq!(stdout.matches("Usage:").count(), 1, "{stdout}");
}

#[test]
fn root_help_falls_back_to_old_toolchain_help() {
    let home = TestHome::new();
    home.write_cli("#!/bin/sh\nif [ \"$1\" = \"__baml-root-help-v1\" ]; then exit 2; fi\nif [ \"$1\" = \"--help\" ]; then printf '%s\\n' 'legacy baml-cli help'; exit 0; fi\nexit 9\n");
    let output = home.run_args(&["--help"]);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "legacy baml-cli help\n"
    );
}

#[test]
fn root_help_falls_back_from_an_unsupported_schema() {
    let home = TestHome::new();
    home.write_cli(
        r#"#!/bin/sh
if [ "$1" = "__baml-root-help-v1" ]; then
  printf '%s\n' '{"schema_version":"baml.root-help.v2","name":"baml","version":"2","about":"BAML","usage":"baml <COMMAND>","commands":[],"options":[]}'
  exit 0
fi
if [ "$1" = "--help" ]; then printf '%s\n' 'newer baml-cli help'; exit 0; fi
exit 9
"#,
    );
    let output = home.run_args(&["--help"]);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "newer baml-cli help\n"
    );
}

#[test]
fn root_help_falls_back_from_malformed_metadata() {
    let home = TestHome::new();
    home.write_cli(
        "#!/bin/sh\nif [ \"$1\" = \"__baml-root-help-v1\" ]; then printf '%s\\n' 'not json'; exit 0; fi\nif [ \"$1\" = \"--help\" ]; then printf '%s\\n' 'fallback help'; exit 0; fi\nexit 9\n",
    );
    let output = home.run_args(&["--help"]);
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "fallback help\n");
}

#[test]
fn subcommand_help_is_forwarded_unchanged() {
    let home = TestHome::new();
    home.write_cli("#!/bin/sh\nprintf '%s\\n' \"$*\"\n");
    let output = home.run_args(&["check", "--help"]);
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "check --help\n");
}

#[test]
fn root_help_works_before_a_toolchain_is_installed() {
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_baml"))
        .arg("--help")
        .env("BAML_HOME", home.path())
        .env("HOME", home.path())
        .env_remove("BAML_VERSION")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Install or select a toolchain"), "{stdout}");
    assert!(stdout.contains("toolchain"), "{stdout}");
    assert!(stdout.contains("self-update"), "{stdout}");
    assert!(!stdout.contains("check"), "{stdout}");
}
