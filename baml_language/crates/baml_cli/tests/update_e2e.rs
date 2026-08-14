use std::process::Command;

#[test]
fn update_suggests_the_toolchain_and_wrapper_commands() {
    let output = Command::new(env!("CARGO_BIN_EXE_baml-cli"))
        .arg("update")
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        // Pin the human preset so inherited agent env (CLAUDECODE/AI_AGENT/…)
        // cannot flip `--output-preset auto` to `agent` and hide progress lines.
        .env("BAML_OUTPUT_PRESET", "human")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("`baml update` is ambiguous."), "{stderr}");
    assert!(
        stderr.contains("To use the latest version of BAML, run `baml toolchain update`."),
        "{stderr}"
    );
    assert!(
        stderr.contains("To use the latest BAML toolchain selector, run `baml self-update`."),
        "{stderr}"
    );
}
