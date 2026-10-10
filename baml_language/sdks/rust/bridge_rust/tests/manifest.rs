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

    let metadata: serde_json_feature_free::Value =
        serde_json_feature_free::from_slice(&output.stdout)
            .expect("cargo metadata should emit JSON");
    let bridge = metadata["packages"]
        .as_array()
        .expect("metadata packages should be an array")
        .iter()
        .find(|package| package["name"] == "baml_bridge")
        .expect("metadata should contain baml_bridge");
    let serde_json_dependencies = bridge["dependencies"]
        .as_array()
        .expect("package dependencies should be an array")
        .iter()
        .filter(|dependency| dependency["name"] == "serde_json")
        .collect::<Vec<_>>();
    assert!(
        serde_json_dependencies
            .iter()
            .any(|dependency| dependency["rename"] == "serde_json_feature_free"),
        "baml_bridge should use the feature-free serde_json alias"
    );

    for dependency in serde_json_dependencies {
        let features = dependency["features"]
            .as_array()
            .expect("dependency features should be an array");
        for unwanted in ["arbitrary_precision", "preserve_order"] {
            assert!(
                !features.iter().any(|feature| feature == unwanted),
                "baml_bridge must not enable serde_json/{unwanted}; Cargo unifies dependency features across consumers"
            );
        }
    }
}
