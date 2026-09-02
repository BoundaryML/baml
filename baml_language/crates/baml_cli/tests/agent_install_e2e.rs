//! End-to-end tests for the toolchain-owned BAML agent skill.

mod common;

use std::{fs, process::Command};

fn run_from(project: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_baml-cli"))
        .args(args)
        .current_dir(project)
        .env("HOME", project.parent().unwrap_or(project))
        .env("DO_NOT_TRACK", "1")
        .env("BAML_AGENT_SKILL_CHECK", "warn")
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .env("NO_PROXY", "")
        .output()
        .unwrap()
}

#[test]
fn init_warns_then_embedded_install_silences_authoring_commands() {
    let tree = tempfile::tempdir().unwrap();
    let project = tree.path().join("project");
    fs::create_dir_all(&project).unwrap();

    let output = run_from(&project, &["init"]);
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no baml skill is installed; set it up with `baml agent install`"),
        "{stderr}"
    );

    let mut baml_toml = fs::read_to_string(project.join("baml.toml")).unwrap();
    baml_toml.push_str(
        "\n[generator.py]\noutput_type = \"python/pydantic\"\noutput_dir = \"generated\"\nnaming_convention = \"preserve-case\"\n",
    );
    fs::write(project.join("baml.toml"), baml_toml).unwrap();

    let output = run_from(&project, &["agent", "install"]);
    assert!(
        output.status.success(),
        "agent install failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = common::installed_skill_content();
    for dir in [".agents/skills", ".claude/skills"] {
        assert_eq!(
            fs::read_to_string(project.join(dir).join("baml-core/SKILL.md")).unwrap(),
            expected
        );
    }

    let output = run_from(&project, &["generate"]);
    assert!(
        output.status.success(),
        "generate failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("baml skill"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn install_is_network_independent_and_archives_the_previous_skill() {
    let project = tempfile::tempdir().unwrap();
    let expected = common::installed_skill_content();
    for dir in [".agents/skills", ".claude/skills"] {
        let skill = project.path().join(dir).join("baml-core/SKILL.md");
        fs::create_dir_all(skill.parent().unwrap()).unwrap();
        fs::write(skill, "old").unwrap();
    }

    let output = run_from(project.path(), &["agent", "install"]);
    assert!(
        output.status.success(),
        "agent install failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    for dir in [".agents/skills", ".claude/skills"] {
        let root = project.path().join(dir);
        assert_eq!(
            fs::read_to_string(root.join("baml-core/SKILL.md")).unwrap(),
            expected
        );
        assert_eq!(
            fs::read_to_string(root.join("baml-old_skills/baml-core/SKILL.md")).unwrap(),
            "old"
        );
    }
}
