//! Shared state and network helpers for BAML agent skills.
//!
//! Agent skills are markdown files installed into user projects from the
//! skill repo (`BoundaryML/baml-skill`) head. Unlike toolchains they have a
//! single track (the repo's `main` branch) and are identified by git commit
//! SHA rather than a released version.
//!
//! This module is the single source of truth for skill state and freshness.
//! The `baml-cli` toolchain binary owns both sides: `agent install` records
//! what it installed, and the passive authoring-command check (`skill_check`
//! in `baml_cli`) surfaces missing/outdated warnings from the caches here.

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

/// Cache file recording the latest known skill repo commit, written by
/// `agent install` and refreshed at most once per TTL window by the
/// toolchain's passive skill check. Lives next to the toolchain manifest
/// cache.
pub fn latest_skill_commit_cache_path() -> PathBuf {
    baml_home()
        .join("manifest-cache")
        .join("skills")
        .join("latest-commit.json")
}

/// How long a cached latest-commit answer stays fresh before a passive
/// network re-check is due.
pub const LATEST_COMMIT_CACHE_TTL: Duration = Duration::from_hours(24);

/// Decide which passive skill warning (if any) applies:
///
/// - No `baml-*` skills in the project at all: prompt to install. This fires
///   regardless of caches, so users discover skills exist.
/// - Skills present but the `[skills]` provenance is missing or behind the
///   cached latest skill repo commit: prompt to upgrade. Requires the cache
///   (written by the auto-check or explicit commands); without it we can't
///   know what's current, so we stay silent rather than guess.
pub fn skill_warning_message(
    project_has_skills: bool,
    state: Option<&SkillsState>,
    cached_latest: Option<&str>,
) -> Option<&'static str> {
    if !project_has_skills {
        return Some("no baml skill is installed; set it up with `baml agent install`");
    }
    let latest = cached_latest?;
    match state {
        Some(state) if state.installed_commit == latest => None,
        _ => Some("your baml skill is outdated; use `baml agent install` to upgrade it"),
    }
}

/// Walk from the current directory up to $HOME looking for installed
/// `baml-*` agent skills (`.agents/skills/` or `.claude/skills/`).
pub fn project_has_baml_skills() -> bool {
    let Ok(mut dir) = std::env::current_dir() else {
        return false;
    };
    let home = std::env::var_os("HOME").map(PathBuf::from);
    loop {
        let agents = dir.join(".agents").join("skills");
        let claude = dir.join(".claude").join("skills");
        if dir_contains_baml_skill(&agents) || dir_contains_baml_skill(&claude) {
            return true;
        }
        if home.as_ref().is_some_and(|home| dir == *home) || !dir.pop() {
            return false;
        }
    }
}

/// A skill counts only when it's a `baml-*` directory actually containing a
/// `SKILL.md`; a leftover empty directory or stray file must not suppress the
/// missing-skill prompt.
fn dir_contains_baml_skill(skills_dir: &Path) -> bool {
    fs::read_dir(skills_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(std::result::Result::ok)
        .any(|entry| {
            entry.file_name().to_string_lossy().starts_with("baml-")
                && entry.path().join("SKILL.md").is_file()
        })
}

/// Whether normal commands may refresh the freshness caches over the network,
/// per `[update] auto_check` in `~/.baml/config.toml`. Defaults to on; only an
/// explicit `auto_check = false` opts out. Shared by the wrapper and the
/// toolchain so one setting silences both.
pub fn update_auto_check_enabled() -> bool {
    let Ok(text) = fs::read_to_string(baml_home().join("config.toml")) else {
        return true;
    };
    let Ok(value) = text.parse::<toml::Value>() else {
        return true;
    };
    value
        .get("update")
        .and_then(|update| update.get("auto_check"))
        .and_then(toml::Value::as_bool)
        .unwrap_or(true)
}

/// The latest-commit cache is due for a refresh attempt when both the cache
/// file and its attempt marker are older than the TTL. The marker is touched
/// before every attempt (success or failure) so an unreachable network is
/// retried at most once per TTL window instead of on every command.
pub fn should_attempt_latest_commit_refresh() -> bool {
    should_attempt_refresh(&latest_skill_commit_cache_path(), LATEST_COMMIT_CACHE_TTL)
}

fn should_attempt_refresh(cache_path: &Path, ttl: Duration) -> bool {
    let marker = refresh_marker_path(cache_path);
    if !file_older_than(cache_path, ttl) || !file_older_than(&marker, ttl) {
        return false;
    }
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&marker, "").is_ok()
}

fn refresh_marker_path(cache_path: &Path) -> PathBuf {
    let mut path = cache_path.as_os_str().to_owned();
    path.push(".last-check");
    PathBuf::from(path)
}

/// True when the file is missing or its mtime is older than `ttl`.
fn file_older_than(path: &Path, ttl: Duration) -> bool {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| std::time::SystemTime::now().duration_since(modified).ok())
        .is_none_or(|age| age > ttl)
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
/// section (notably the wrapper-owned `[channels.*]`). Only a missing file is
/// treated as empty; an unreadable or unparseable file is an error, so a
/// corrupt `state.toml` is never silently replaced (which would also destroy
/// the other writers' sections).
pub fn write_skills_state(state_path: &Path, skills: &SkillsState) -> Result<()> {
    let mut root = read_state_document(state_path)?;
    let table = root
        .as_table_mut()
        .context("state.toml root is not a table")?;
    table.insert(
        "skills".to_string(),
        toml::Value::try_from(skills).context("failed to serialize skills state")?,
    );
    write_state_document(state_path, &root)
}

/// Remove the `[skills]` section of `state.toml`, preserving every other
/// section. Used after installs from custom sources, which have no commit
/// identity: stale provenance would otherwise make the wrapper report
/// unrelated content as current.
pub fn clear_skills_state(state_path: &Path) -> Result<()> {
    let mut root = read_state_document(state_path)?;
    let Some(table) = root.as_table_mut() else {
        return Ok(());
    };
    if table.remove("skills").is_none() {
        return Ok(());
    }
    write_state_document(state_path, &root)
}

fn read_state_document(state_path: &Path) -> Result<toml::Value> {
    let text = match fs::read_to_string(state_path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(toml::Value::Table(toml::value::Table::new()));
        }
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read {}", state_path.display()));
        }
    };
    text.parse::<toml::Value>().with_context(|| {
        format!(
            "failed to parse {} (fix or delete the file and retry)",
            state_path.display()
        )
    })
}

fn write_state_document(state_path: &Path, root: &toml::Value) -> Result<()> {
    let text = toml::to_string_pretty(root).context("failed to serialize state.toml")?;
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
/// one, while the passive background check should use a short one.
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
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
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
    // Both values are provably in range (day 1..=31, month 1..=12); the
    // fallbacks only exist to satisfy the no-panicking-cast lints.
    let day = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
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
    fn write_skills_state_refuses_to_clobber_corrupt_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.toml");
        fs::write(&path, "this is not [valid toml").unwrap();

        let skills = SkillsState {
            installed_commit: "abc123".to_string(),
            installed_at: "2026-07-10T00:00:00Z".to_string(),
        };
        let err = write_skills_state(&path, &skills).unwrap_err();
        assert!(format!("{err:#}").contains("failed to parse"), "{err:#}");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "this is not [valid toml"
        );
    }

    #[test]
    fn clear_skills_state_removes_section_and_preserves_others() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.toml");
        fs::write(
            &path,
            "[channels.canary]\nactive_version = \"0.11.0\"\nresolved_at = \"x\"\nmanifest_path = \"y\"\n\n[skills]\ninstalled_commit = \"abc\"\ninstalled_at = \"z\"\n",
        )
        .unwrap();

        clear_skills_state(&path).unwrap();

        assert!(read_skills_state(&path).is_none());
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("[channels.canary]"), "{text}");

        // Missing file and missing section are both no-ops.
        clear_skills_state(&path).unwrap();
        clear_skills_state(&tmp.path().join("missing.toml")).unwrap();
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

    fn skills_state(commit: &str) -> SkillsState {
        SkillsState {
            installed_commit: commit.to_string(),
            installed_at: "x".to_string(),
        }
    }

    #[test]
    fn missing_project_skills_prompt_install_regardless_of_caches() {
        let expected = Some("no baml skill is installed; set it up with `baml agent install`");
        assert_eq!(skill_warning_message(false, None, None), expected);
        assert_eq!(skill_warning_message(false, None, Some("bbb")), expected);
        // Even matching global provenance doesn't matter: this project has no skills.
        assert_eq!(
            skill_warning_message(false, Some(&skills_state("bbb")), Some("bbb")),
            expected
        );
    }

    #[test]
    fn skills_behind_or_untracked_prompt_upgrade() {
        let expected = Some("your baml skill is outdated; use `baml agent install` to upgrade it");
        assert_eq!(
            skill_warning_message(true, Some(&skills_state("aaa")), Some("bbb")),
            expected
        );
        assert_eq!(skill_warning_message(true, None, Some("bbb")), expected);
    }

    #[test]
    fn current_skills_or_unknown_latest_stay_silent() {
        assert_eq!(
            skill_warning_message(true, Some(&skills_state("bbb")), Some("bbb")),
            None
        );
        // No cached latest: can't judge freshness, so no nag.
        assert_eq!(
            skill_warning_message(true, Some(&skills_state("aaa")), None),
            None
        );
        assert_eq!(skill_warning_message(true, None, None), None);
    }

    #[test]
    fn refresh_attempt_is_throttled_by_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("latest-commit.json");
        // Cache missing and no marker: attempt allowed, marker gets created.
        assert!(should_attempt_refresh(&cache, LATEST_COMMIT_CACHE_TTL));
        assert!(refresh_marker_path(&cache).exists());
        // Fresh marker: no retry within the TTL window.
        assert!(!should_attempt_refresh(&cache, LATEST_COMMIT_CACHE_TTL));
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
