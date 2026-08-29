//! End-to-end tests for local embedded-skill freshness warnings.

mod common;

use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

const SKILL_OUTDATED_WARNING: &str =
    "your baml skill does not match this toolchain; use `baml agent install` to upgrade it";
const SKILL_MISSING_WARNING: &str =
    "no baml skill is installed; set it up with `baml agent install`";

fn project_with_skill(content: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let skill_dir = dir.path().join(".agents/skills/baml-core");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    dir
}

fn run_args_from(args: &[&str], cwd: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_baml-cli"))
        .args(args)
        .current_dir(cwd)
        .env("HOME", cwd.parent().unwrap_or(cwd))
        .env("BAML_WRAPPER_EXEC", "1")
        .env("DO_NOT_TRACK", "1")
        .output()
        .unwrap()
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn non_authoring_commands_never_warn() {
    let empty = tempfile::tempdir().unwrap();

    for command in [["check"].as_slice(), ["fmt"].as_slice()] {
        let stderr = stderr_of(&run_args_from(command, empty.path()));
        assert!(!stderr.contains(SKILL_MISSING_WARNING), "{stderr}");
    }
}

#[test]
fn missing_project_skill_prompts_install() {
    let empty = tempfile::tempdir().unwrap();
    let stderr = stderr_of(&run_args_from(&["generate"], empty.path()));

    assert!(stderr.contains(SKILL_MISSING_WARNING), "{stderr}");
    assert!(!stderr.contains(SKILL_OUTDATED_WARNING), "{stderr}");
}

#[test]
fn empty_skill_directory_still_prompts_install() {
    let project = tempfile::tempdir().unwrap();
    fs::create_dir_all(project.path().join(".agents/skills/baml-core")).unwrap();

    let stderr = stderr_of(&run_args_from(&["generate"], project.path()));
    assert!(stderr.contains(SKILL_MISSING_WARNING), "{stderr}");
}

#[test]
fn archived_old_skill_does_not_count_as_installed() {
    let project = tempfile::tempdir().unwrap();
    let archived = project
        .path()
        .join(".claude/skills/baml-old_skills/baml-core");
    fs::create_dir_all(&archived).unwrap();
    fs::write(archived.join("SKILL.md"), "old").unwrap();

    let stderr = stderr_of(&run_args_from(&["generate"], project.path()));
    assert!(stderr.contains(SKILL_MISSING_WARNING), "{stderr}");
}

#[test]
fn stale_project_skill_prompts_upgrade() {
    let project = project_with_skill("old");
    let stderr = stderr_of(&run_args_from(&["generate"], project.path()));

    assert!(stderr.contains(SKILL_OUTDATED_WARNING), "{stderr}");
    assert!(!stderr.contains(SKILL_MISSING_WARNING), "{stderr}");
}

#[test]
fn matching_project_skill_is_silent() {
    let project = project_with_skill(&common::installed_skill_content());
    let stderr = stderr_of(&run_args_from(&["generate"], project.path()));

    assert!(!stderr.contains("baml skill"), "{stderr}");
}

#[test]
fn matching_project_skill_takes_precedence_over_stale_parent() {
    let tree = tempfile::tempdir().unwrap();
    let parent_skill = tree.path().join(".agents/skills/baml-core/SKILL.md");
    fs::create_dir_all(parent_skill.parent().unwrap()).unwrap();
    fs::write(parent_skill, "old").unwrap();
    let project = tree.path().join("project");
    let project_skill = project.join(".agents/skills/baml-core/SKILL.md");
    fs::create_dir_all(project_skill.parent().unwrap()).unwrap();
    fs::write(project_skill, common::installed_skill_content()).unwrap();

    let stderr = stderr_of(&run_args_from(&["generate"], &project));
    assert!(!stderr.contains("baml skill"), "{stderr}");
}

#[test]
fn stale_copy_in_either_agent_directory_prompts_upgrade() {
    let project = project_with_skill(&common::installed_skill_content());
    let skill = project.path().join(".claude/skills/baml-core/SKILL.md");
    fs::create_dir_all(skill.parent().unwrap()).unwrap();
    fs::write(skill, "old").unwrap();

    let stderr = stderr_of(&run_args_from(&["generate"], project.path()));
    assert!(stderr.contains(SKILL_OUTDATED_WARNING), "{stderr}");
}

#[test]
fn agent_command_never_nags() {
    let empty = tempfile::tempdir().unwrap();
    let stderr = stderr_of(&run_args_from(&["agent", "install"], empty.path()));

    assert!(!stderr.contains(SKILL_MISSING_WARNING), "{stderr}");
}
