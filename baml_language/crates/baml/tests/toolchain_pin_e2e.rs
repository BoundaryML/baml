//! End-to-end tests for project-local toolchain pinning.

use std::{fs, path::Path, process::Command};

fn install_fake_toolchain(home: &Path, version: &str) {
    let bin = home.join("toolchains").join(version).join("bin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(
        home.join("toolchains").join(version).join("VERSION"),
        format!("{version}\n"),
    )
    .unwrap();
    fs::write(
        bin.join(if cfg!(windows) {
            "baml-cli.exe"
        } else {
            "baml-cli"
        }),
        "",
    )
    .unwrap();
}

fn cache_channel_manifest(home: &Path, channel: &str, version: &str) {
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
    let cache = home.join("manifest-cache/prod");
    fs::create_dir_all(&cache).unwrap();
    fs::write(
        cache.join(format!("{channel}.json")),
        serde_json::to_string(&serde_json::json!({
            "schema": 1,
            "version": version,
            "channel": channel,
            "released_at": "2026-08-12T00:00:00Z",
            "artifacts": artifacts,
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn pin_updates_nearest_manifest_and_replaces_channel() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let project = temp.path().join("project");
    let nested = project.join("baml_src/nested");
    fs::create_dir_all(&nested).unwrap();
    fs::create_dir_all(&home).unwrap();
    install_fake_toolchain(&home, "0.16.0");
    fs::write(
        project.join("baml.toml"),
        "[package]\nname = \"demo\"\n\n[toolchain]\n# selected for CI\nchannel = \"nightly\"\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_baml"))
        .args(["toolchain", "pin", "0.16.0"])
        .current_dir(&nested)
        .env("BAML_HOME", &home)
        .env("HOME", temp.path())
        .env_remove("BAML_VERSION")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("pinned BAML toolchain 0.16.0"), "{stdout}");
    assert!(
        stdout.contains(&project.join("baml.toml").display().to_string()),
        "{stdout}"
    );

    let manifest = fs::read_to_string(project.join("baml.toml")).unwrap();
    assert!(manifest.contains("# selected for CI"), "{manifest}");
    assert!(manifest.contains("version = \"0.16.0\""), "{manifest}");
    assert!(!manifest.contains("channel ="), "{manifest}");
}

#[test]
fn pin_accepts_canary_and_nightly_channels() {
    for channel in ["canary", "nightly"] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let project = temp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&home).unwrap();
        install_fake_toolchain(&home, "0.16.0");
        cache_channel_manifest(&home, channel, "0.16.0");
        fs::write(
            project.join("baml.toml"),
            "[toolchain]\nversion = \"0.15.0\"\n",
        )
        .unwrap();

        let output = Command::new(env!("CARGO_BIN_EXE_baml"))
            .args(["toolchain", "pin", channel])
            .current_dir(&project)
            .env("BAML_HOME", &home)
            .env("HOME", temp.path())
            .env_remove("BAML_VERSION")
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "{}: {}",
            channel,
            String::from_utf8_lossy(&output.stderr)
        );
        let manifest = fs::read_to_string(project.join("baml.toml")).unwrap();
        assert!(
            manifest.contains(&format!("channel = \"{channel}\"")),
            "{manifest}"
        );
        assert!(!manifest.contains("version ="), "{manifest}");
        let state = fs::read_to_string(home.join("state.toml")).unwrap();
        assert!(state.contains(&format!("[channels.{channel}]")), "{state}");
        assert!(state.contains("active_version = \"0.16.0\""), "{state}");
    }
}

#[test]
fn pin_accepts_and_activates_a_local_path() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let project = temp.path().join("project");
    let nested = project.join("baml_src/nested");
    let local_cli_name = if cfg!(windows) {
        "baml-cli.exe"
    } else {
        "baml-cli"
    };
    let local_cli = nested.join("local").join(local_cli_name);
    let local_selector = format!("./local/{local_cli_name}");
    fs::create_dir_all(local_cli.parent().unwrap()).unwrap();
    fs::create_dir_all(&home).unwrap();
    fs::write(&local_cli, "").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&local_cli, fs::Permissions::from_mode(0o755)).unwrap();
    }
    fs::write(
        project.join("baml.toml"),
        "[toolchain]\nversion = \"0.16.0\"\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_baml"))
        .args(["toolchain", "pin", &local_selector])
        .current_dir(&nested)
        .env("BAML_HOME", &home)
        .env("HOME", temp.path())
        .env_remove("BAML_VERSION")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest = fs::read_to_string(project.join("baml.toml")).unwrap();
    let expected_cli = nested
        .canonicalize()
        .unwrap()
        .join("local")
        .join(local_cli_name);
    let manifest_value = manifest.parse::<toml::Value>().unwrap();
    let toolchain = manifest_value["toolchain"].as_table().unwrap();
    assert_eq!(
        toolchain["path"].as_str(),
        Some(expected_cli.to_str().unwrap()),
        "{manifest}"
    );
    assert!(!toolchain.contains_key("version"), "{manifest}");

    let status = Command::new(env!("CARGO_BIN_EXE_baml"))
        .args(["toolchain", "status"])
        .current_dir(&nested)
        .env("BAML_HOME", &home)
        .env("HOME", temp.path())
        .env_remove("BAML_VERSION")
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(
        stdout.contains(&format!("active selector: {}", expected_cli.display())),
        "{stdout}"
    );
    let expected_manifest = project.canonicalize().unwrap().join("baml.toml");
    assert!(
        stdout.contains(&format!("source: set by {}", expected_manifest.display())),
        "{stdout}"
    );
    assert!(
        stdout.contains("Remote versions were not checked."),
        "{stdout}"
    );
}
