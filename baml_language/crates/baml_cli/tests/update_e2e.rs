//! Updates remain CLI commands; they describe the two independent update paths.
use std::process::Command;

#[test]
fn update_and_upgrade_only_print_guidance() {
    let home = tempfile::tempdir().unwrap();
    for command in ["update", "upgrade"] {
        let output = Command::new(env!("CARGO_BIN_EXE_baml-cli"))
            .arg(command)
            .current_dir(home.path())
            .env("BAML_HOME", home.path().join("baml"))
            .env("BAML_CLI_ALLOW_DIRECT", "1")
            .env("DO_NOT_TRACK", "1")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let text = String::from_utf8(output.stdout).unwrap();
        for expected in [
            "independent updates",
            "baml self-update",
            "baml toolchain update",
            "Exact-version and local-path selections do not advance",
            "No update was performed",
        ] {
            assert!(text.contains(expected), "{command}: {text}");
        }
        assert!(!home.path().join("baml/toolchains").exists());
        assert!(!home.path().join("baml/config.toml").exists());
    }
}

#[test]
fn directory_flag_is_removed() {
    let output = Command::new(env!("CARGO_BIN_EXE_baml-cli"))
        .args(["--directory", ".", "check"])
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("DO_NOT_TRACK", "1")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unexpected argument '--directory'"));
}
