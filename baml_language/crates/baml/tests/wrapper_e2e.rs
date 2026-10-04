//! Run the BAML wrapper against local toolchains and configuration.
#![cfg(unix)]

use std::{
    fs,
    io::Write as _,
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    project: PathBuf,
    home: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let project = root.join("project");
        let home = root.join("baml-home");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&home).unwrap();
        fs::write(home.join("config.toml"), "# user config\n[default] # keep table\nselector = '0.11.0'\nextra = true\n[update]\nauto_check = false\n[other]\nvalue = 42\n").unwrap();
        let fixture = Self {
            _temp: temp,
            root,
            project,
            home,
        };
        fixture.install("0.11.0");
        fixture.install("0.12.0");
        fixture
    }
    fn script(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "#!/bin/sh\nprintf '%s|%s|%s\\n' \"$BAML_WRAPPER_EXEC\" \"${BAML_WRAPPER_RESOLVED_TOOLCHAIN-unset}\" \"${BAML_WRAPPER_LOCAL_TOOLCHAIN-unset}\"\nprintf '%s\\n' \"$@\"\ncat\nexit 19\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    fn install(&self, version: &str) {
        let root = self.home.join("toolchains").join(version);
        Self::script(&root.join("bin/baml-cli"));
        fs::write(root.join("VERSION"), format!("{version}\n")).unwrap();
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_baml"));
        cmd.args(args)
            .current_dir(&self.project)
            .env("BAML_HOME", &self.home)
            .env("HOME", &self.root)
            .env_remove("BAML_VERSION")
            .env_remove("BAML_MANIFEST_BASE_URL");
        cmd
    }
    fn success(&self, args: &[&str]) -> Output {
        let out = self.command(args).output().unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        out
    }
    fn cache_channel(&self, version: &str) {
        let artifacts: serde_json::Map<String, serde_json::Value> = ["aarch64-apple-darwin", "x86_64-apple-darwin", "aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu", "aarch64-unknown-linux-musl", "x86_64-unknown-linux-musl", "aarch64-pc-windows-msvc", "x86_64-pc-windows-msvc"]
            .into_iter().map(|target| (target.into(), serde_json::json!({ "url": format!("https://example.invalid/{target}"), "sha256": "0".repeat(64) }))).collect();
        let cache = self.home.join("manifest-cache/prod");
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("canary.json"), serde_json::json!({"schema":1,"version":version,"channel":"canary","released_at":"now","artifacts":artifacts}).to_string()).unwrap();
    }
}

#[test]
fn selector_precedence_and_child_preserve_arguments_streams_and_status() {
    let f = Fixture::new();
    fs::write(
        f.project.join("baml.toml"),
        "[toolchain]\nversion = '0.12.0'\n",
    )
    .unwrap();
    let out = f
        .command(&["hello", "--literal", "a b"])
        .env("BAML_WRAPPER_LOCAL_TOOLCHAIN", "stale")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(19));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "1|0.12.0|unset\nhello\n--literal\na b\n"
    );
    let local = f.project.join("local-cli");
    Fixture::script(&local);
    let mut child = f
        .command(&["hello", "--flag", "a b"])
        .env("BAML_VERSION", "  ./local-cli  ")
        .env("BAML_WRAPPER_RESOLVED_TOOLCHAIN", "stale")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"stdin payload")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(19));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!(
            "1|unset|{}\nhello\n--flag\na b\nstdin payload",
            local.display()
        )
    );
    assert!(out.stderr.is_empty());
}

#[test]
fn use_and_pin_local_cli_preserve_configuration_and_comments() {
    let f = Fixture::new();
    Fixture::script(&f.project.join("local-cli"));
    f.success(&["toolchain", "use", "./local-cli"]);
    let config = fs::read_to_string(f.home.join("config.toml")).unwrap();
    let value: toml::Value = toml::from_str(&config).unwrap();
    assert_eq!(
        value["default"]["selector"].as_str(),
        Some(f.project.join("local-cli").to_str().unwrap())
    );
    assert_eq!(value["other"]["value"].as_integer(), Some(42));
    assert!(config.contains("# user config"));
    assert!(config.contains("# keep table"));
    fs::write(f.project.join("baml.toml"), "# project\n[package]\nname = 'app'\n[toolchain] # selected\nversion = 'old' # keep comment\nextra = true\n").unwrap();
    f.success(&["toolchain", "pin", "./local-cli"]);
    let config = fs::read_to_string(f.project.join("baml.toml")).unwrap();
    let value: toml::Value = toml::from_str(&config).unwrap();
    assert_eq!(
        value["toolchain"]["path"].as_str(),
        Some(f.project.join("local-cli").to_str().unwrap())
    );
    assert!(value["toolchain"].get("version").is_none());
    assert_eq!(value["toolchain"]["extra"].as_bool(), Some(true));
    assert!(
        config.contains("# project")
            && config.contains("# selected")
            && config.contains("# keep comment")
    );
    // An inline table can contain stale competing selectors. Pinning removes
    // them and explicitly renames the highest-priority selector's comment.
    fs::write(f.project.join("baml.toml"), "# project\ntoolchain = { version = 'old', channel = 'stale', path = 'unused', extra = true } # inline\n[package]\nname = 'app'\n").unwrap();
    f.success(&["toolchain", "pin", "./local-cli"]);
    let config = fs::read_to_string(f.project.join("baml.toml")).unwrap();
    let value: toml::Value = toml::from_str(&config).unwrap();
    assert_eq!(
        value["toolchain"]["path"].as_str(),
        Some(f.project.join("local-cli").to_str().unwrap())
    );
    assert!(value["toolchain"].get("version").is_none());
    assert!(value["toolchain"].get("channel").is_none());
    assert_eq!(value["toolchain"]["extra"].as_bool(), Some(true));
    assert!(
        config.contains("toolchain = {")
            && config.contains("# inline")
            && config.contains("# project")
    );
}

#[test]
fn cached_channel_activation_and_local_listing_need_no_network() {
    let f = Fixture::new();
    f.cache_channel("0.11.0");
    fs::write(f.home.join("state.toml"), "[foreign]\nkeep = 'yes'\n").unwrap();
    f.success(&["toolchain", "use", "canary"]);
    let state: toml::Value =
        toml::from_str(&fs::read_to_string(f.home.join("state.toml")).unwrap()).unwrap();
    assert_eq!(
        state["channels"]["canary"]["active_version"].as_str(),
        Some("0.11.0")
    );
    assert_eq!(state["foreign"]["keep"].as_str(), Some("yes"));
    let out = f.success(&["toolchain", "list"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("default: canary")
            && text.contains("0.11.0")
            && text.contains("0.12.0")
            && text.contains("Remote versions were not checked.")
    );
    let out = f.success(&["--version"]);
    let manifest: toml::Value = toml::from_str(include_str!("../Cargo.toml")).unwrap();
    let wrapper_version = manifest["package"]["version"].as_str().unwrap();
    assert!(
        String::from_utf8_lossy(&out.stdout).contains(&format!("baml wrapper {wrapper_version}"))
    );
    f.success(&["toolchain", "uninstall", "0.12.0"]);
    assert!(!f.home.join("toolchains/0.12.0").exists());
}

#[test]
fn invalid_toolchains_and_self_recursion_fail_before_launch() {
    let f = Fixture::new();
    fs::write(f.home.join("toolchains/0.11.0/VERSION"), "wrong\n").unwrap();
    let out = f.command(&["hello"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--force"));
    let out = f
        .command(&["hello"])
        .env("BAML_VERSION", env!("CARGO_BIN_EXE_baml"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("wrapper itself"));
    let out = f
        .command(&["toolchain", "uninstall", "../outside"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Invalid toolchain version"));
}

#[test]
fn project_search_stops_at_a_symlinked_home_boundary() {
    let f = Fixture::new();
    let user_home = f.root.join("users/home");
    let cwd = user_home.join("project");
    fs::create_dir_all(&cwd).unwrap();
    let link = f.root.join("linked-home");
    std::os::unix::fs::symlink(&user_home, &link).unwrap();
    fs::write(
        f.root.join("users/baml.toml"),
        "[toolchain]\nversion = '0.12.0'\n",
    )
    .unwrap();
    let out = f
        .command(&["hello"])
        .current_dir(cwd)
        .env("HOME", link)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(19));
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("1|0.11.0|unset\n"));
}

#[test]
fn local_version_uses_successful_first_line_and_handles_failure() {
    let f = Fixture::new();
    let local = f.project.join("local-cli");
    Fixture::script(&local);
    fs::write(
        &local,
        "#!/bin/sh\nprintf 'baml-cli 1.2.3\\nextra diagnostics\\n'\n",
    )
    .unwrap();
    let out = f
        .command(&["--version"])
        .env("BAML_VERSION", &local)
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("baml toolchain 1.2.3 (local:"));
    assert!(!stdout.contains("extra diagnostics"));
    fs::write(&local, "#!/bin/sh\nprintf 'baml-cli invalid\\n'\nexit 1\n").unwrap();
    let out = f
        .command(&["--version"])
        .env("BAML_VERSION", &local)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("baml toolchain version unknown"));
}
