use std::{fs, process::Command};

const MAIN_GUIDE: &str = include_str!("../../../../skill/guides/main.md");
const SKILL_STUB: &str = include_str!("../../../../skill/stub.md");

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

#[test]
fn install_copies_the_embedded_stub_without_network_state() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();

    let output = baml_command()
        .args(["agent", "install", "--project"])
        .arg(project.path())
        .env("BAML_HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "agent install failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    for skills_dir in [".agents/skills", ".claude/skills"] {
        let installed = project.path().join(skills_dir).join("baml/SKILL.md");
        assert_eq!(fs::read_to_string(installed).unwrap(), SKILL_STUB);
    }
    assert!(!home.path().join("state.toml").exists());
    assert!(!home.path().join("manifest-cache/skills").exists());
}

#[test]
fn install_migrates_baml_core_and_is_idempotent() {
    let project = tempfile::tempdir().unwrap();
    for skills_dir in [".agents/skills", ".claude/skills"] {
        let legacy = project.path().join(skills_dir).join("baml-core/SKILL.md");
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(legacy, "legacy").unwrap();
    }

    for _ in 0..2 {
        let output = baml_command()
            .args(["agent", "install", "--project"])
            .arg(project.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "agent install failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    for skills_dir in [".agents/skills", ".claude/skills"] {
        let root = project.path().join(skills_dir);
        assert!(!root.join("baml-core").exists());
        assert_eq!(
            fs::read_to_string(root.join("baml/SKILL.md")).unwrap(),
            SKILL_STUB
        );
        assert_eq!(
            fs::read_to_string(root.join("baml-old_skills/baml-core/SKILL.md")).unwrap(),
            "legacy"
        );
    }
}
