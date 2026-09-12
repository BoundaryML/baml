use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use serde_json::Value;

struct TestHome {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl TestHome {
    fn new(default_selector: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::write(
            root.join("config.toml"),
            format!("[default]\nselector = {default_selector:?}\n"),
        )
        .unwrap();
        Self { _dir: dir, root }
    }

    fn install(&self, version: &str) -> PathBuf {
        let bin = self.root.join("toolchains").join(version).join("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(
            self.root.join("toolchains").join(version).join("VERSION"),
            format!("{version}\n"),
        )
        .unwrap();
        let cli = bin.join(if cfg!(windows) {
            "baml-cli.exe"
        } else {
            "baml-cli"
        });
        fs::copy(env!("CARGO_BIN_EXE_baml"), &cli).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&cli, fs::Permissions::from_mode(0o755)).unwrap();
        }
        cli
    }

    fn activate_channel(&self, channel: &str, version: &str) {
        fs::write(
            self.root.join("state.toml"),
            format!(
                "[channels.{channel}]\nactive_version = {version:?}\nresolved_at = \"test\"\nmanifest_path = \"test\"\n"
            ),
        )
        .unwrap();
    }

    fn cache_channel_manifest(&self, channel: &str, version: &str) {
        let artifacts = baml_release::SUPPORTED_RELEASE_TARGETS
            .iter()
            .map(|target| {
                (
                    (*target).to_string(),
                    serde_json::json!({
                        "url": format!("https://example.test/{target}.tar.gz"),
                        "sha256": "0".repeat(64),
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let cache = self.root.join("manifest-cache/prod");
        fs::create_dir_all(&cache).unwrap();
        fs::write(
            cache.join(format!("{channel}.json")),
            serde_json::to_vec(&serde_json::json!({
                "schema": 1,
                "version": version,
                "channel": channel,
                "released_at": "2026-08-18T00:00:00Z",
                "artifacts": artifacts,
            }))
            .unwrap(),
        )
        .unwrap();
    }

    fn run(&self, cwd: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_baml"))
            .args(args)
            .current_dir(cwd)
            .env("BAML_HOME", &self.root)
            .env("HOME", cwd.parent().unwrap_or(cwd))
            .env_remove("BAML_VERSION")
            .output()
            .unwrap()
    }
}

fn json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "wrapper failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "stdout was not JSON: {err}: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

#[test]
fn resolves_active_channel_as_json() {
    let home = TestHome::new("canary");
    let cli = home.install("0.11.0");
    home.activate_channel("canary", "0.11.0");
    let cwd = tempfile::tempdir().unwrap();

    let resolution = json(&home.run(cwd.path(), &["toolchain", "resolve", "--json"]));

    assert_eq!(resolution["selector"], "canary");
    assert_eq!(resolution["version"], "0.11.0");
    assert_eq!(resolution["toolchain_path"], cli.display().to_string());
    assert_eq!(resolution["installed"], true);
    let reported_wrapper = Path::new(resolution["wrapper_path"].as_str().unwrap());
    assert_eq!(
        fs::canonicalize(reported_wrapper).unwrap(),
        fs::canonicalize(env!("CARGO_BIN_EXE_baml")).unwrap()
    );
}

#[test]
fn install_flag_keeps_stdout_machine_readable_for_installed_exact_version() {
    let home = TestHome::new("0.11.0");
    home.install("0.11.0");
    let cwd = tempfile::tempdir().unwrap();

    let resolution = json(&home.run(cwd.path(), &["toolchain", "resolve", "--install", "--json"]));

    assert_eq!(resolution["selector"], "0.11.0");
    assert_eq!(resolution["version"], "0.11.0");
    assert_eq!(resolution["installed"], true);
}

#[test]
#[cfg(unix)]
fn non_executable_managed_binary_is_not_reported_as_installed() {
    use std::os::unix::fs::PermissionsExt;

    let home = TestHome::new("0.11.0");
    let cli = home.install("0.11.0");
    fs::set_permissions(&cli, fs::Permissions::from_mode(0o644)).unwrap();
    let cwd = tempfile::tempdir().unwrap();

    let resolution = json(&home.run(cwd.path(), &["toolchain", "resolve", "--json"]));

    assert_eq!(resolution["version"], "0.11.0");
    assert_eq!(resolution["installed"], false);
}

#[test]
#[cfg(windows)]
fn invalid_windows_managed_binary_is_not_reported_as_installed() {
    let home = TestHome::new("0.11.0");
    let cli = home.install("0.11.0");
    fs::write(cli, "").unwrap();
    let cwd = tempfile::tempdir().unwrap();

    let resolution = json(&home.run(cwd.path(), &["toolchain", "resolve", "--json"]));

    assert_eq!(resolution["version"], "0.11.0");
    assert_eq!(resolution["installed"], false);
}

#[test]
fn install_flag_resolves_and_activates_channel_without_prior_state() {
    let home = TestHome::new("canary");
    home.install("0.12.0");
    home.cache_channel_manifest("canary", "0.12.0");
    let cwd = tempfile::tempdir().unwrap();

    let output = home.run(cwd.path(), &["toolchain", "resolve", "--install", "--json"]);
    let resolution = json(&output);

    assert_eq!(resolution["selector"], "canary");
    assert_eq!(resolution["version"], "0.12.0");
    assert_eq!(resolution["installed"], true);
    assert!(Path::new(resolution["toolchain_path"].as_str().unwrap()).exists());
    let state = fs::read_to_string(home.root.join("state.toml")).unwrap();
    assert!(state.contains("active_version = \"0.12.0\""), "{state}");
}

#[test]
fn install_flag_preserves_recorded_channel_version() {
    let home = TestHome::new("canary");
    home.install("0.11.0");
    home.activate_channel("canary", "0.11.0");
    home.cache_channel_manifest("canary", "0.12.0");
    let cwd = tempfile::tempdir().unwrap();

    let output = home.run(cwd.path(), &["toolchain", "resolve", "--install", "--json"]);
    let resolution = json(&output);

    assert_eq!(resolution["selector"], "canary");
    assert_eq!(resolution["version"], "0.11.0");
    assert_eq!(resolution["installed"], true);
    let state = fs::read_to_string(home.root.join("state.toml")).unwrap();
    assert!(state.contains("active_version = \"0.11.0\""), "{state}");
    assert!(!state.contains("active_version = \"0.12.0\""), "{state}");
}

#[test]
fn environment_override_wins_over_project_and_global_selectors() {
    let home = TestHome::new("0.10.0");
    let project = tempfile::tempdir().unwrap();
    fs::write(
        project.path().join("baml.toml"),
        "[toolchain]\nversion = \"0.11.0\"\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_baml"))
        .args(["toolchain", "resolve", "--json"])
        .current_dir(project.path())
        .env("BAML_HOME", &home.root)
        .env("HOME", project.path().parent().unwrap())
        .env("BAML_VERSION", "0.12.0")
        .output()
        .unwrap();

    let resolution = json(&output);
    assert_eq!(resolution["selector"], "0.12.0");
    assert_eq!(resolution["version"], "0.12.0");
    assert_eq!(resolution["installed"], false);
}

#[test]
fn project_path_selector_is_rejected_with_source_attribution() {
    let home = TestHome::new("canary");
    let project = tempfile::tempdir().unwrap();
    let manifest = project.path().join("baml.toml");
    fs::write(
        &manifest,
        "[toolchain]\npath = \"./target/debug/baml-cli\"\n",
    )
    .unwrap();

    let output = home.run(project.path(), &["toolchain", "resolve", "--json"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("local path"), "{stderr}");
    assert!(stderr.contains(&manifest.display().to_string()), "{stderr}");
}
