use std::{path::PathBuf, process::Command};

#[test]
fn serde_json_dependency_does_not_force_semantic_features() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--locked",
            "--manifest-path",
        ])
        .arg(manifest)
        .output()
        .expect("cargo metadata should run");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("cargo metadata should emit JSON");
    let bridge = metadata["packages"]
        .as_array()
        .expect("metadata packages should be an array")
        .iter()
        .find(|package| package["name"] == "baml_bridge")
        .expect("metadata should contain baml_bridge");
    let serde_json = bridge["dependencies"]
        .as_array()
        .expect("package dependencies should be an array")
        .iter()
        .find(|dependency| dependency["name"] == "serde_json")
        .expect("baml_bridge should depend on serde_json");
    let features = serde_json["features"]
        .as_array()
        .expect("dependency features should be an array");

    for unwanted in ["arbitrary_precision", "preserve_order"] {
        assert!(
            !features.iter().any(|feature| feature == unwanted),
            "baml_bridge must not enable serde_json/{unwanted}; Cargo unifies dependency features across consumers"
        );
    }
}
