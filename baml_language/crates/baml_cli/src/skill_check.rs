//! Validation for missing or stale BAML agent skills.
//!
//! The active toolchain compares the installed skill's frontmatter version
//! with its own version. This keeps skill freshness local and gives each
//! toolchain version its matching instructions without separate version state.

use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::{agent_command::SKILL_NAME, output::AgentSkillCheckPolicy};

const SKILL_OUTDATED_WARNING: &str =
    "your baml skill does not match this toolchain; use `baml agent install` to upgrade it";
const SKILL_MISSING_WARNING: &str =
    "no baml skill is installed; set it up with `baml agent install`";
const SKILL_OUTDATED_ERROR: &str = "the installed BAML agent skill does not match this toolchain; run `baml agent install`, restart the agent, then retry; set BAML_AGENT_SKILL_CHECK=off to bypass this check";
const SKILL_MISSING_ERROR: &str = "the BAML agent skill is required but is not installed; run `baml agent install`, restart the agent, then retry; set BAML_AGENT_SKILL_CHECK=off to bypass this check";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SkillStatus {
    Missing,
    Current,
    Outdated,
}

#[derive(Deserialize)]
struct SkillFrontmatter {
    metadata: SkillMetadata,
}

#[derive(Deserialize)]
struct SkillMetadata {
    #[serde(rename = "baml-toolchain-version")]
    toolchain_version: String,
}

pub(crate) fn check(project: Option<&Path>) -> anyhow::Result<()> {
    let policy = crate::output::policy().agent_skill_check;
    if policy == AgentSkillCheckPolicy::Off {
        return Ok(());
    }

    let status = project_skill_status(project)?;
    match (policy, status) {
        (_, SkillStatus::Current) => Ok(()),
        (AgentSkillCheckPolicy::Warn, status) => {
            if let Some(message) = skill_warning_message(status) {
                crate::reporter::print_warning(format_args!("{message}"));
            }
            Ok(())
        }
        (AgentSkillCheckPolicy::Require, SkillStatus::Missing) => {
            anyhow::bail!(SKILL_MISSING_ERROR)
        }
        (AgentSkillCheckPolicy::Require, SkillStatus::Outdated) => {
            anyhow::bail!(SKILL_OUTDATED_ERROR)
        }
        (AgentSkillCheckPolicy::Off, _) => Ok(()),
    }
}

fn skill_warning_message(status: SkillStatus) -> Option<&'static str> {
    match status {
        SkillStatus::Missing => Some(SKILL_MISSING_WARNING),
        SkillStatus::Outdated => Some(SKILL_OUTDATED_WARNING),
        SkillStatus::Current => None,
    }
}

fn project_skill_status(project: Option<&Path>) -> anyhow::Result<SkillStatus> {
    let mut dir = match project {
        Some(project) => skill_search_start(project)?,
        None => match std::env::current_dir() {
            Ok(dir) => dir,
            Err(_) => return Ok(SkillStatus::Missing),
        },
    };
    let home = std::env::var_os("HOME").map(PathBuf::from);

    loop {
        let statuses = [".agents/skills", ".claude/skills"]
            .map(|relative| installed_skill_status(&dir.join(relative)));
        if statuses.contains(&SkillStatus::Outdated) {
            return Ok(SkillStatus::Outdated);
        }
        if statuses.contains(&SkillStatus::Current) {
            return Ok(SkillStatus::Current);
        }
        if home.as_ref().is_some_and(|home| dir == *home) || !dir.pop() {
            break;
        }
    }

    Ok(SkillStatus::Missing)
}

/// Resolve the closest existing directory at or above a command's project path.
/// `init` and `new` may target directories that do not exist until after the
/// preflight check, so project discovery cannot require the target itself to
/// canonicalize yet.
fn skill_search_start(project: &Path) -> anyhow::Result<PathBuf> {
    let mut candidate = if project.is_absolute() {
        project.to_path_buf()
    } else {
        std::env::current_dir()?.join(project)
    };

    loop {
        match std::fs::canonicalize(&candidate) {
            Ok(path) => return Ok(baml_db::project_search_dir(&path)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound && candidate.pop() => {}
            Err(err) => return Err(err.into()),
        }
    }
}

fn installed_skill_status(skills_dir: &Path) -> SkillStatus {
    let path = skills_dir.join(SKILL_NAME).join("SKILL.md");
    // TODO: This opens SKILL.md on every checked CLI invocation. Add caching
    // if it becomes measurable.
    match installed_toolchain_version(&path) {
        Ok(Some(version)) if version == baml_version::CANONICAL_VERSION => SkillStatus::Current,
        Ok(_) => SkillStatus::Outdated,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => SkillStatus::Missing,
        Err(_) => SkillStatus::Outdated,
    }
}

fn installed_toolchain_version(path: &Path) -> std::io::Result<Option<String>> {
    let mut lines = BufReader::new(fs::File::open(path)?).lines();
    if !matches!(lines.next().transpose()?, Some(line) if line == "---") {
        return Ok(None);
    }

    let mut frontmatter = String::new();
    for line in lines {
        let line = line?;
        if line == "---" {
            return Ok(serde_yaml::from_str::<SkillFrontmatter>(&frontmatter)
                .ok()
                .map(|parsed| parsed.metadata.toolchain_version));
        }
        frontmatter.push_str(&line);
        frontmatter.push('\n');
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warning_tracks_local_skill_status() {
        assert_eq!(
            skill_warning_message(SkillStatus::Missing),
            Some(SKILL_MISSING_WARNING)
        );
        assert_eq!(
            skill_warning_message(SkillStatus::Outdated),
            Some(SKILL_OUTDATED_WARNING)
        );
        assert_eq!(skill_warning_message(SkillStatus::Current), None);
    }

    #[test]
    fn installed_skill_status_compares_toolchain_versions() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(installed_skill_status(tmp.path()), SkillStatus::Missing);

        let skill_dir = tmp.path().join(SKILL_NAME);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "old").unwrap();
        assert_eq!(installed_skill_status(tmp.path()), SkillStatus::Outdated);

        fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                "---\nname: baml-core\nmetadata:\n  baml-toolchain-version: {:?}\n---\nchanged body\n",
                baml_version::CANONICAL_VERSION
            ),
        )
        .unwrap();
        assert_eq!(installed_skill_status(tmp.path()), SkillStatus::Current);

        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: baml-core\nmetadata:\n  baml-toolchain-version: old\n---\n",
        )
        .unwrap();
        assert_eq!(installed_skill_status(tmp.path()), SkillStatus::Outdated);
    }

    #[test]
    fn skill_search_uses_existing_parent_for_new_project_path() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("new").join("project");

        assert_eq!(
            skill_search_start(&target).unwrap(),
            std::fs::canonicalize(tmp.path()).unwrap(),
        );
    }
}
