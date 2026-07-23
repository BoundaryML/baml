#![cfg(any(not(feature = "self-update"), feature = "no-self-update"))]

use std::process::Command;

#[test]
fn self_update_is_disabled_without_network_access() {
    let output = Command::new(env!("CARGO_BIN_EXE_baml"))
        .arg("self-update")
        .env(
            "BAML_MANIFEST_BASE_URL",
            "http://127.0.0.1:1/should-not-be-requested",
        )
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "self-update is disabled in this build.\nUpdate BAML with your package manager.\n"
    );
}
