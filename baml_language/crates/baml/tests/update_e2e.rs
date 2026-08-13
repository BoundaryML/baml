use std::process::Command;

#[test]
fn update_suggests_the_toolchain_command_before_resolving_a_toolchain() {
    let output = Command::new(env!("CARGO_BIN_EXE_baml"))
        .arg("update")
        .env("BAML_VERSION", "missing-test-toolchain")
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "`baml update` is ambiguous.\n\
To use the latest version of BAML, run `baml toolchain update`.\n\
To update the BAML wrapper itself, run `baml self-update`.\n"
    );
}
