use std::process::Command;

const MAIN_GUIDE: &str = include_str!("../../../../skill/guides/main.md");

fn baml_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_baml-cli"));
    command.env("DO_NOT_TRACK", "1");
    command
}

#[test]
fn guide_defaults_to_main() {
    let default = baml_command().args(["agent", "guide"]).output().unwrap();
    assert!(
        default.status.success(),
        "agent guide failed: {}",
        String::from_utf8_lossy(&default.stderr)
    );
    assert_eq!(default.stdout, MAIN_GUIDE.as_bytes());

    let explicit = baml_command()
        .args(["agent", "guide", "main"])
        .output()
        .unwrap();
    assert!(explicit.status.success());
    assert_eq!(explicit.stdout, default.stdout);
}

#[test]
fn guide_rejects_unknown_names() {
    let output = baml_command()
        .args(["agent", "guide", "unknown"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("available guides: main"), "{stderr}");
}
