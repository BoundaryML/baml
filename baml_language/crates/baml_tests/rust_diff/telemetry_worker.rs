//! Run the volume-sensitive reader optimized even under ordinary `cargo test`.
use std::{
    path::{Path, PathBuf},
    process::Command,
};

pub fn build(build: &Path, workspace: &Path) -> PathBuf {
    let dir = build.join("telemetry-reader");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let workspace_manifest: toml::Value = std::fs::read_to_string(workspace.join("Cargo.toml"))
        .unwrap()
        .parse()
        .unwrap();
    let mut dependencies = toml::map::Map::new();
    for name in ["prost", "serde", "serde_json", "sha2"] {
        dependencies.insert(
            name.into(),
            workspace_manifest["workspace"]["dependencies"][name].clone(),
        );
    }
    dependencies
        .get_mut("serde")
        .unwrap()
        .as_table_mut()
        .unwrap()
        .insert("features".into(), toml::Value::Array(vec!["derive".into()]));
    dependencies
        .get_mut("serde_json")
        .unwrap()
        .as_table_mut()
        .unwrap()
        .insert(
            "features".into(),
            toml::Value::Array(vec!["arbitrary_precision".into(), "preserve_order".into()]),
        );
    for name in ["btel_reader", "btel_snapshot"] {
        dependencies.insert(
            name.into(),
            toml::Value::Table(
                [(
                    String::from("path"),
                    toml::Value::String(workspace.join("crates").join(name).display().to_string()),
                )]
                .into_iter()
                .collect(),
            ),
        );
    }
    let manifest = format!(
        "[package]\nname = \"baml-differential-telemetry\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n[profile.release]\nopt-level = 2\ndebug = 0\n\n{}",
        toml::to_string(&toml::Value::Table(
            [("dependencies".into(), toml::Value::Table(dependencies))]
                .into_iter()
                .collect()
        ))
        .unwrap()
    );
    write(&dir.join("Cargo.toml"), &manifest);
    write(
        &dir.join("src/telemetry.rs"),
        &include_str!("telemetry.rs").replace(
            "concat!(env!(\"OUT_DIR\"), \"/baml.btel.recording.v2.rs\")",
            "\"wire.rs\"",
        ),
    );
    write(
        &dir.join("src/wire.rs"),
        include_str!(concat!(env!("OUT_DIR"), "/baml.btel.recording.v2.rs")),
    );
    write(
        &dir.join("src/main.rs"),
        r#"mod telemetry;
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    assert_eq!(args.len(), 3);
    std::fs::write(&args[2], telemetry::read(std::path::Path::new(&args[1]))).unwrap();
}
"#,
    );
    std::fs::copy(workspace.join("Cargo.lock"), dir.join("Cargo.lock")).unwrap();
    let target = dir.join("target");
    let status = Command::new("cargo")
        .args(["build", "--release", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(&target)
        .current_dir(workspace)
        .status()
        .unwrap();
    assert!(status.success(), "build optimized telemetry reader");
    target.join("release").join(format!(
        "baml-differential-telemetry{}",
        std::env::consts::EXE_SUFFIX
    ))
}

fn write(path: &Path, source: &str) {
    if std::fs::read_to_string(path).ok().as_deref() != Some(source) {
        std::fs::write(path, source).unwrap();
    }
}
