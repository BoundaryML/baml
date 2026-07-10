//! Shared state and network helpers for BAML agent skills.
//!
//! Agent skills are markdown files installed into user projects from the
//! skill repo (`BoundaryML/baml-skill`) head. Unlike toolchains they have a
//! single track (the repo's `main` branch) and are identified by git commit
//! SHA rather than a released version.
//!
//! This module is the single source of truth shared by the `baml` wrapper
//! (which surfaces "skill outdated" warnings) and the `baml-cli` toolchain
//! binary (whose `agent install` command records what it installed).

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::baml_home;

pub const DEFAULT_SKILL_REPO: &str = "BoundaryML/baml-skill";

/// The GitHub repo skills are installed from, `BAML_AGENT_SKILLS_REPO`
/// overridable for forks and tests.
pub fn skill_repo() -> String {
    std::env::var("BAML_AGENT_SKILLS_REPO")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_SKILL_REPO.to_string())
}

/// URL that resolves the current head commit of the skill repo's main branch.
/// `BAML_AGENT_SKILLS_COMMITS_URL` overrides the full URL (for tests and
/// mirrors); the response must be GitHub-commit-shaped JSON: `{"sha": "..."}`.
pub fn skill_commits_url() -> String {
    std::env::var("BAML_AGENT_SKILLS_COMMITS_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("https://api.github.com/repos/{}/commits/main", skill_repo()))
}

/// Tarball URL for the skill repo at a specific commit (or branch name).
/// `BAML_AGENT_SKILLS_ARCHIVE_BASE_URL` overrides the codeload host (for
/// tests and mirrors); the commit ref is appended as the final path segment.
pub fn skill_archive_url(commit_ref: &str) -> String {
    if let Some(base) = std::env::var("BAML_AGENT_SKILLS_ARCHIVE_BASE_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return format!("{}/{commit_ref}", base.trim_end_matches('/'));
    }
    format!(
        "https://codeload.github.com/{}/tar.gz/{commit_ref}",
        skill_repo()
    )
}

/// `~/.baml/state.toml`: the file recording resolved toolchain channel
/// versions (owned by the wrapper) and installed skill provenance (written by
/// `baml agent install`). Skills use read-modify-write on the raw TOML so the
/// two writers never clobber each other's sections.
pub fn state_path() -> PathBuf {
    baml_home().join("state.toml")
}

/// Cache file recording the latest known skill repo commit, refreshed at most
/// once per TTL window by the wrapper's auto-check and by explicit toolchain
/// commands. Lives next to the toolchain manifest cache.
pub fn latest_skill_commit_cache_path() -> PathBuf {
    baml_home()
        .join("manifest-cache")
        .join("skills")
        .join("latest-commit.json")
}

/// Installed skill provenance, stored as the `[skills]` section of
/// `state.toml`, mirroring how `[channels.<name>]` records the resolved
/// toolchain version:
///
/// ```toml
/// [skills]
/// installed_commit = "abc123..."
/// installed_at = "2026-07-10T18:30:00Z"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsState {
    pub installed_commit: String,
    pub installed_at: String,
}

pub fn read_skills_state(state_path: &Path) -> Option<SkillsState> {
    let text = fs::read_to_string(state_path).ok()?;
    let value = text.parse::<toml::Value>().ok()?;
    value.get("skills")?.clone().try_into().ok()
}

/// Write the `[skills]` section of `state.toml`, preserving every other
/// section (notably the wrapper-owned `[channels.*]`).
pub fn write_skills_state(state_path: &Path, skills: &SkillsState) -> Result<()> {
    let mut root = fs::read_to_string(state_path)
        .ok()
        .and_then(|text| text.parse::<toml::Value>().ok())
        .unwrap_or_else(|| toml::Value::Table(toml::value::Table::new()));
    let table = root
        .as_table_mut()
        .context("state.toml root is not a table")?;
    table.insert(
        "skills".to_string(),
        toml::Value::try_from(skills).context("failed to serialize skills state")?,
    );
    let text = toml::to_string_pretty(&root).context("failed to serialize state.toml")?;
    write_text_atomic(state_path, &text)
}

#[derive(Debug, Serialize, Deserialize)]
struct LatestSkillCommitCache {
    sha: String,
}

pub fn read_cached_latest_skill_commit(cache_path: &Path) -> Option<String> {
    let text = fs::read_to_string(cache_path).ok()?;
    let cache: LatestSkillCommitCache = serde_json::from_str(&text).ok()?;
    Some(cache.sha)
}

pub fn write_cached_latest_skill_commit(cache_path: &Path, sha: &str) -> Result<()> {
    let text = serde_json::to_string(&LatestSkillCommitCache {
        sha: sha.to_string(),
    })?;
    write_text_atomic(cache_path, &text)
}

/// Resolve the latest commit SHA of the skill repo's main branch over the
/// network. Callers decide the timeout: explicit commands can afford a long
/// one, while the wrapper's passive auto-check should use a short one.
pub fn fetch_latest_skill_commit(timeout: Duration) -> Result<String> {
    let url = skill_commits_url();
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(timeout.min(Duration::from_secs(10)))
        .timeout(timeout)
        .user_agent("baml-skill-check/1")
        .build()
        .context("failed to build HTTP client")?;
    let response = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .with_context(|| format!("failed to fetch {url}"))?
        .error_for_status()
        .with_context(|| format!("failed to fetch {url}"))?;
    let text = response
        .text()
        .with_context(|| format!("failed to read {url}"))?;
    let commit: LatestSkillCommitCache = serde_json::from_str(&text)
        .with_context(|| format!("{url} did not return commit JSON with a sha field"))?;
    if commit.sha.trim().is_empty() {
        anyhow::bail!("{url} returned an empty commit sha");
    }
    Ok(commit.sha)
}

fn write_text_atomic(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text).with_context(|| format!("failed to write {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}

/// Current UTC time as an RFC 3339 timestamp (second precision), used for the
/// `installed_at` provenance field without pulling in a datetime dependency.
pub fn utc_now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let time_of_day = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time_of_day / 3600,
        (time_of_day % 3600) / 60,
        time_of_day % 60
    )
}

/// Convert days since 1970-01-01 to a (year, month, day) civil date.
/// Howard Hinnant's `civil_from_days` algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skills_state_roundtrip_preserves_other_sections() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.toml");
        fs::write(
            &path,
            "[channels.canary]\nactive_version = \"0.11.0\"\nresolved_at = \"2026-07-01T00:00:00Z\"\nmanifest_path = \"x\"\n",
        )
        .unwrap();

        let skills = SkillsState {
            installed_commit: "abc123".to_string(),
            installed_at: "2026-07-10T00:00:00Z".to_string(),
        };
        write_skills_state(&path, &skills).unwrap();

        let read = read_skills_state(&path).unwrap();
        assert_eq!(read.installed_commit, "abc123");
        assert_eq!(read.installed_at, "2026-07-10T00:00:00Z");

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("[channels.canary]"), "{text}");
        assert!(text.contains("active_version = \"0.11.0\""), "{text}");
    }

    #[test]
    fn skills_state_missing_file_or_section_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("missing.toml");
        assert!(read_skills_state(&missing).is_none());

        let path = tmp.path().join("state.toml");
        fs::write(&path, "[channels.canary]\nactive_version = \"1\"\nresolved_at = \"x\"\nmanifest_path = \"y\"\n").unwrap();
        assert!(read_skills_state(&path).is_none());
    }

    #[test]
    fn latest_commit_cache_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested").join("latest-commit.json");
        assert!(read_cached_latest_skill_commit(&path).is_none());
        write_cached_latest_skill_commit(&path, "def456").unwrap();
        assert_eq!(
            read_cached_latest_skill_commit(&path).as_deref(),
            Some("def456")
        );
    }

    #[test]
    fn utc_now_rfc3339_shape() {
        let now = utc_now_rfc3339();
        assert_eq!(now.len(), 20, "{now}");
        assert!(now.ends_with('Z'), "{now}");
        assert_eq!(&now[4..5], "-");
        assert_eq!(&now[10..11], "T");
    }

    #[test]
    fn civil_from_days_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // 2026-07-10 is 20644 days after the epoch.
        assert_eq!(civil_from_days(20_644), (2026, 7, 10));
        // Leap day.
        assert_eq!(civil_from_days(18_321), (2020, 2, 29));
    }
}
