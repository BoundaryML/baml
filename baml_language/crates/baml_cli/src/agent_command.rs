use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read, Write},
    path::{Component, Path, PathBuf},
    process::Command,
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use clap::Args;

use crate::{ExitCode, project_load::find_project_root_from};

const HTTP_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Args, Clone, Debug)]
pub(crate) struct AgentArgs {
    #[command(subcommand)]
    pub command: AgentCommand,
}

#[derive(clap::Subcommand, Clone, Debug)]
pub(crate) enum AgentCommand {
    #[command(about = "Install or refresh the latest BAML agent skills in this project")]
    Install(AgentInstallArgs),
}

/// Install or refresh the latest BAML agent skills in this project.
///
/// On install, each skill's `name:` frontmatter field is prefixed with `baml-`
/// (so upstream `core` is installed as `baml-core`). This namespaces the skills
/// to avoid collisions in the agent skill registry, and is the only difference
/// from the upstream skill files.
#[derive(Args, Clone, Debug)]
#[command(
    after_long_help = "Note: skill names are prefixed with 'baml-' on install to avoid registry collisions (e.g. upstream 'core' becomes 'baml-core')."
)]
pub(crate) struct AgentInstallArgs {
    /// Directory where project-local agent skills should be installed.
    ///
    /// When omitted, BAML installs at the nearest ancestor with baml.toml,
    /// then the git root, then the current directory.
    #[arg(long, value_name = "PATH")]
    pub dir: Option<PathBuf>,

    /// Install skills from a tar.gz URL, local tar.gz archive, or local directory.
    #[arg(long, value_name = "URL_OR_PATH")]
    pub from: Option<String>,
}

#[derive(Debug)]
struct Skill {
    name: String,
    content: String,
}

#[derive(Debug)]
struct RawSkill {
    name: String,
    legacy_name: Option<String>,
    content: String,
    source_path: PathBuf,
}

enum SkillArchivePath {
    Direct { name: String },
    Legacy { legacy_name: String, name: String },
}

impl AgentArgs {
    pub fn run(&self) -> Result<ExitCode> {
        match &self.command {
            AgentCommand::Install(args) => args.run(),
        }
    }
}

impl AgentInstallArgs {
    pub fn run(&self) -> Result<ExitCode> {
        let root = match &self.dir {
            Some(dir) => explicit_install_root(dir)?,
            None => detect_install_root()?,
        };
        let loaded = load_skills(self)?;
        install_skills(&root, &loaded.skills)?;
        match &loaded.commit {
            Some(commit) => record_installed_commit(commit),
            None => clear_installed_commit(),
        }
        print_success(&root)?;
        Ok(ExitCode::Success)
    }
}

#[derive(Debug)]
struct LoadedSkills {
    skills: Vec<Skill>,
    /// Commit of the skill repo the installed content came from, recovered
    /// from the tarball's pax global header (`comment=<sha>`, written by
    /// `git archive` and present in GitHub codeload tarballs). Present only
    /// when installing from the default source; custom `--from` sources have
    /// no commit identity to record, and archives without the header (or with
    /// a non-SHA comment) install fine with no recorded provenance.
    commit: Option<String>,
}

fn load_skills(args: &AgentInstallArgs) -> Result<LoadedSkills> {
    if let Some(source) = &args.from {
        if is_http_url(source) {
            let archive = fetch_url_bytes(source)?;
            return Ok(LoadedSkills {
                skills: skills_from_archive(&archive)?.skills,
                commit: None,
            });
        }

        let path = Path::new(source);
        if path.is_dir() {
            return Ok(LoadedSkills {
                skills: skills_from_dir(path)?,
                commit: None,
            });
        }

        let archive = fs::read(path).with_context(|| {
            format!(
                "failed to read BAML agent skills archive {}",
                path.display()
            )
        })?;
        return Ok(LoadedSkills {
            skills: skills_from_archive(&archive)?.skills,
            commit: None,
        });
    }

    // Default: download the main-branch tarball from codeload. Deliberately
    // NOT the GitHub REST API: the tarball endpoint needs no credentials and
    // is not subject to the unauthenticated 60-requests/hour API rate limit
    // that used to hard-block installs. The tarball embeds the commit it was
    // cut from in its pax global header, which becomes the recorded
    // provenance; a mirror that omits the header just skips provenance.
    let url = baml_release::skills::skill_archive_url("main");
    let archive = fetch_url_bytes(&url).with_context(|| {
        format!(
            "could not download the BAML agent skills from {url}; if GitHub is \
             unreachable, install from a local copy with \
             `baml agent install --from <url-or-path>`"
        )
    })?;
    let loaded = skills_from_archive(&archive)?;
    Ok(LoadedSkills {
        skills: loaded.skills,
        commit: loaded.commit,
    })
}

/// Record which skill commit was installed (in `~/.baml/state.toml`) and
/// refresh the latest-commit cache so freshness warnings clear immediately.
/// Failures are reported but don't fail the install: the skills themselves
/// were written successfully.
fn record_installed_commit(commit: &str) {
    let state = baml_release::skills::SkillsState {
        installed_commit: commit.to_string(),
        installed_at: baml_release::skills::utc_now_rfc3339(),
    };
    if let Err(err) =
        baml_release::skills::write_skills_state(&baml_release::skills::state_path(), &state)
    {
        crate::reporter::print_warning(format_args!(
            "failed to record installed skill commit: {err:#}"
        ));
    }
    if let Err(err) = baml_release::skills::write_cached_latest_skill_commit(
        &baml_release::skills::latest_skill_commit_cache_path(),
        commit,
    ) {
        crate::reporter::print_warning(format_args!(
            "failed to update skill freshness cache: {err:#}"
        ));
    }
}

/// Installs with no commit identity (custom `--from` sources, or a default
/// archive whose pax header carried no commit) drop any previously recorded
/// provenance so the wrapper doesn't report the installed content as up to
/// date with (or behind) the official skill repo.
fn clear_installed_commit() {
    if let Err(err) = baml_release::skills::clear_skills_state(&baml_release::skills::state_path())
    {
        crate::reporter::print_warning(format_args!(
            "failed to clear recorded skill provenance: {err:#}"
        ));
    }
}

fn is_http_url(value: &str) -> bool {
    value.starts_with("https://") || value.starts_with("http://")
}

fn fetch_url_bytes(url: &str) -> Result<Vec<u8>> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(HTTP_TIMEOUT)
        .user_agent("baml-agent-install/1")
        .build()
        .context("failed to build HTTP client")?;

    let response = client
        .get(url)
        .send()
        .with_context(|| format!("failed to fetch {url}"))?;
    if !response.status().is_success() {
        anyhow::bail!(
            "failed to fetch BAML agent skills from {url}: HTTP {}",
            response.status()
        );
    }
    Ok(response
        .bytes()
        .context("failed to read BAML agent skills archive")?
        .to_vec())
}

fn explicit_install_root(dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    dir.canonicalize()
        .with_context(|| format!("failed to resolve {}", dir.display()))
}

fn detect_install_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("failed to read current directory")?;
    let canonical = cwd
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", cwd.display()))?;

    if let Some(root) = find_project_root_from(Some(&canonical))? {
        return Ok(root);
    }

    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(&canonical)
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let root = stdout.trim();
            if !root.is_empty() {
                return Ok(PathBuf::from(root));
            }
        }
    }

    Ok(canonical)
}

fn skills_from_archive(archive: &[u8]) -> Result<LoadedSkills> {
    let decoder = flate2::read::GzDecoder::new(Cursor::new(archive));
    let mut archive = tar::Archive::new(decoder);
    let mut raw = Vec::new();
    let mut commit = None;

    for entry in archive.entries().context("failed to read skill archive")? {
        let mut entry = entry.context("failed to read skill archive entry")?;
        if entry.header().entry_type() == tar::EntryType::XGlobalHeader {
            if commit.is_none() {
                commit = pax_comment_sha(&mut entry);
            }
            continue;
        }
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry
            .path()
            .context("failed to read skill archive entry path")?
            .into_owned();
        let parts = normalized_components(&path)?;
        let Some(skill_path) = skill_archive_path(&parts) else {
            continue;
        };

        let mut content = String::new();
        entry
            .read_to_string(&mut content)
            .with_context(|| format!("failed to read {}", path.display()))?;

        raw.push(raw_skill(skill_path, content, path));
    }

    Ok(LoadedSkills {
        skills: normalize_skills(raw)?,
        commit,
    })
}

/// Extract the commit SHA a codeload tarball was cut from: `git archive`
/// records it as a `comment=<sha>` record in the pax global header entry.
/// Returns `None` (rather than erroring) for archives without the header or
/// with a comment that doesn't look like a git SHA, so custom mirrors and
/// hand-rolled archives still install — they just record no provenance.
fn pax_comment_sha(entry: &mut impl Read) -> Option<String> {
    let mut body = Vec::new();
    entry.take(4096).read_to_end(&mut body).ok()?;
    let comment = pax_records(&body).find_map(|(key, value)| (key == "comment").then_some(value))?;
    let sha = comment.trim();
    let looks_like_sha =
        (7..=64).contains(&sha.len()) && sha.chars().all(|c| c.is_ascii_hexdigit());
    looks_like_sha.then(|| sha.to_string())
}

/// Iterate `<len> <key>=<value>\n` records in a pax extended header body,
/// skipping anything malformed.
fn pax_records(body: &[u8]) -> impl Iterator<Item = (&str, &str)> {
    let mut rest = body;
    std::iter::from_fn(move || {
        loop {
            let text = std::str::from_utf8(rest).ok()?;
            let (len_text, _) = text.split_once(' ')?;
            let record_len: usize = len_text.parse().ok()?;
            if record_len <= len_text.len() + 1 {
                return None;
            }
            let record = text.get(..record_len)?;
            rest = &rest[record_len..];
            let content = &record[len_text.len() + 1..];
            if let Some((key, value)) = content.trim_end_matches('\n').split_once('=') {
                return Some((key, value));
            }
        }
    })
}

fn skills_from_dir(root: &Path) -> Result<Vec<Skill>> {
    let mut skill_files = Vec::new();
    collect_skill_files(root, &mut skill_files)?;

    let mut raw = Vec::new();
    for path in skill_files {
        let relative = path
            .strip_prefix(root)
            .with_context(|| format!("failed to relativize {}", path.display()))?;
        let parts = normalized_components(relative)?;
        let Some(skill_path) = skill_archive_path(&parts) else {
            continue;
        };
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        raw.push(raw_skill(skill_path, content, relative.to_path_buf()));
    }

    normalize_skills(raw)
}

fn collect_skill_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry.with_context(|| format!("failed to read {}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", path.display()))?;
        if file_type.is_dir() {
            collect_skill_files(&path, files)?;
        } else if file_type.is_file() && path.file_name().is_some_and(|name| name == "SKILL.md") {
            files.push(path);
        }
    }
    Ok(())
}

fn raw_skill(skill_path: SkillArchivePath, content: String, source_path: PathBuf) -> RawSkill {
    match skill_path {
        SkillArchivePath::Direct { name } => RawSkill {
            name,
            legacy_name: None,
            content,
            source_path,
        },
        SkillArchivePath::Legacy { legacy_name, name } => RawSkill {
            name,
            legacy_name: Some(legacy_name),
            content,
            source_path,
        },
    }
}

fn normalize_skills(raw: Vec<RawSkill>) -> Result<Vec<Skill>> {
    if raw.is_empty() {
        anyhow::bail!(
            "BAML agent skills source did not contain any skills/baml-*/SKILL.md entries"
        );
    }

    let mut found = BTreeMap::<String, RawSkill>::new();
    for skill in raw {
        if let Some(previous) = found.insert(skill.name.clone(), skill) {
            anyhow::bail!(
                "BAML agent skills source contains duplicate skill `{}` at {}",
                previous.name,
                previous.source_path.display()
            );
        }
    }

    let skill_names = found.keys().cloned().collect::<Vec<_>>();
    found
        .into_values()
        .map(|skill| {
            let content = if skill.legacy_name.is_some() {
                normalize_legacy_skill_content(&skill.content, &skill.name).with_context(|| {
                    format!("failed to normalize {}", skill.source_path.display())
                })?
            } else {
                validate_skill_name(&skill.content, &skill.name).with_context(|| {
                    format!("failed to validate {}", skill.source_path.display())
                })?;
                skill.content
            };
            Ok(Skill {
                name: skill.name,
                content: normalize_skill_references(content, &skill_names),
            })
        })
        .collect()
}

fn normalized_components(path: &Path) -> Result<Vec<String>> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                anyhow::bail!("skill archive contains unsafe path {}", path.display());
            }
        }
    }
    Ok(parts)
}

fn skill_archive_path(parts: &[String]) -> Option<SkillArchivePath> {
    if parts.len() >= 3
        && parts[parts.len() - 3] == "skills"
        && parts[parts.len() - 1] == "SKILL.md"
    {
        let name = parts[parts.len() - 2].clone();
        if name.starts_with("baml-") {
            return Some(SkillArchivePath::Direct { name });
        }
    }

    if parts.len() >= 5
        && parts[parts.len() - 5] == "plugins"
        && parts[parts.len() - 4] == "baml"
        && parts[parts.len() - 3] == "skills"
        && parts[parts.len() - 1] == "SKILL.md"
    {
        let legacy_name = parts[parts.len() - 2].clone();
        if !legacy_name.is_empty() {
            let name = format!("baml-{legacy_name}");
            return Some(SkillArchivePath::Legacy { legacy_name, name });
        }
    }

    None
}

fn normalize_legacy_skill_content(content: &str, name: &str) -> Result<String> {
    let (frontmatter, body) = split_frontmatter(content)?;
    let frontmatter = replace_frontmatter_name(frontmatter, name)?;
    Ok(format!("---\n{frontmatter}---\n{body}"))
}

fn normalize_skill_references(mut content: String, skill_names: &[String]) -> String {
    for skill_name in skill_names {
        if let Some(legacy_name) = skill_name.strip_prefix("baml-") {
            content = content.replace(&format!("baml:{legacy_name}"), skill_name);
        }
    }
    content = content.replace("baml:*", "baml-*");
    content
}

fn split_frontmatter(content: &str) -> Result<(&str, &str)> {
    let rest = content
        .strip_prefix("---\n")
        .or_else(|| content.strip_prefix("---\r\n"))
        .ok_or_else(|| anyhow!("SKILL.md is missing opening frontmatter marker"))?;
    let Some((closing_start, closing_marker)) = ["\n---\n", "\r\n---\r\n"]
        .into_iter()
        .filter_map(|marker| rest.find(marker).map(|index| (index, marker)))
        .min_by_key(|(index, _)| *index)
    else {
        anyhow::bail!("SKILL.md is missing closing frontmatter marker");
    };
    Ok((
        &rest[..closing_start],
        &rest[closing_start + closing_marker.len()..],
    ))
}

fn validate_skill_name(content: &str, expected_name: &str) -> Result<()> {
    let (frontmatter, _) = split_frontmatter(content)?;
    let got = frontmatter_name(frontmatter)
        .ok_or_else(|| anyhow!("SKILL.md frontmatter is missing `name`"))?;
    if got != expected_name {
        anyhow::bail!("SKILL.md frontmatter name must be `{expected_name}`, got `{got}`");
    }
    Ok(())
}

fn frontmatter_name(frontmatter: &str) -> Option<String> {
    frontmatter.lines().find_map(|line| {
        let value = line.trim().strip_prefix("name:")?.trim();
        Some(value.trim_matches('"').trim_matches('\'').to_string())
    })
}

fn replace_frontmatter_name(frontmatter: &str, name: &str) -> Result<String> {
    let mut replaced = false;
    let mut out = String::new();
    for line in frontmatter.lines() {
        if line.trim().starts_with("name:") {
            out.push_str("name: ");
            out.push_str(name);
            out.push('\n');
            replaced = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !replaced {
        anyhow::bail!("SKILL.md frontmatter is missing `name`");
    }
    Ok(out)
}

fn install_skills(root: &Path, skills: &[Skill]) -> Result<()> {
    install_skills_to(root, Path::new(".agents").join("skills"), skills)?;
    install_skills_to(root, Path::new(".claude").join("skills"), skills)?;
    Ok(())
}

fn install_skills_to(root: &Path, relative_skills_dir: PathBuf, skills: &[Skill]) -> Result<()> {
    let skills_dir = root.join(relative_skills_dir);
    fs::create_dir_all(&skills_dir)
        .with_context(|| format!("failed to create {}", skills_dir.display()))?;

    let tmp_dir = skills_dir.join(format!(".baml-agent-install-{}", std::process::id()));
    if tmp_dir.exists() {
        fs::remove_dir_all(&tmp_dir)
            .with_context(|| format!("failed to remove stale {}", tmp_dir.display()))?;
    }
    fs::create_dir_all(&tmp_dir)
        .with_context(|| format!("failed to create {}", tmp_dir.display()))?;

    let result = (|| -> Result<()> {
        for skill in skills {
            let skill_dir = tmp_dir.join(&skill.name);
            fs::create_dir_all(&skill_dir)
                .with_context(|| format!("failed to create {}", skill_dir.display()))?;
            write_atomic(&skill_dir.join("SKILL.md"), &skill.content)?;
        }

        for skill in skills {
            replace_skill_dir(&skills_dir, &tmp_dir, skill)?;
        }
        Ok(())
    })();

    let cleanup = fs::remove_dir_all(&tmp_dir);
    match (result, cleanup) {
        (Err(err), _) => Err(err),
        (Ok(()), Err(err)) if err.kind() != std::io::ErrorKind::NotFound => {
            Err(err).context("failed to clean up temporary BAML agent skill directory")
        }
        (Ok(()), _) => Ok(()),
    }
}

fn replace_skill_dir(skills_dir: &Path, tmp_dir: &Path, skill: &Skill) -> Result<()> {
    let final_dir = skills_dir.join(&skill.name);
    let next_dir = tmp_dir.join(&skill.name);
    let backup_dir = unique_backup_dir(skills_dir, &skill.name)?;
    let mut has_backup = false;

    if final_dir.exists() {
        fs::rename(&final_dir, &backup_dir)
            .with_context(|| format!("failed to stage existing {}", final_dir.display()))?;
        has_backup = true;
    }

    if let Err(err) = fs::rename(&next_dir, &final_dir) {
        let mut error = anyhow!(err).context(format!(
            "failed to install {} into {}",
            skill.name,
            final_dir.display()
        ));
        if has_backup {
            if final_dir.exists() {
                error = error.context(format!(
                    "previous {} skill remains at {}",
                    skill.name,
                    backup_dir.display()
                ));
            } else if let Err(restore_err) = fs::rename(&backup_dir, &final_dir) {
                error = error.context(format!(
                    "failed to restore previous {} skill from {} to {}: {restore_err}",
                    skill.name,
                    backup_dir.display(),
                    final_dir.display()
                ));
            }
        }
        return Err(error);
    }

    if has_backup {
        fs::remove_dir_all(&backup_dir)
            .with_context(|| format!("failed to remove backup {}", backup_dir.display()))?;
    }

    Ok(())
}

fn unique_backup_dir(skills_dir: &Path, skill_name: &str) -> Result<PathBuf> {
    for attempt in 0..1000 {
        let backup_dir = skills_dir.join(format!(
            ".baml-agent-install-backup-{}-{skill_name}-{attempt}",
            std::process::id()
        ));
        if !backup_dir.exists() {
            return Ok(backup_dir);
        }
    }
    anyhow::bail!("failed to find available backup directory for {skill_name}");
}

fn write_atomic(path: &Path, content: &str) -> Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut file = fs::File::create(&tmp)
            .with_context(|| format!("failed to create {}", tmp.display()))?;
        file.write_all(content.as_bytes())
            .with_context(|| format!("failed to write {}", tmp.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync {}", tmp.display()))?;
    }
    fs::rename(&tmp, path).with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}

/// Build the post-install summary text shown after a successful install.
///
/// The message lists the destination glob paths and discloses the `baml-`
/// name-prefix transformation so it is documented rather than silent.
///
/// # Parameters
/// - `root`: the project root the skills were installed under.
///
/// # Returns
/// The summary string (without a trailing newline).
fn success_message(root: &Path) -> String {
    format!(
        "Installed BAML agent skills in {}\n\nClaude Code:\n  .claude/skills/baml-*/SKILL.md\n\nCodex / OpenCode:\n  .agents/skills/baml-*/SKILL.md\n\nNote: skill names are prefixed with 'baml-' on install to avoid registry collisions (e.g. upstream 'core' becomes 'baml-core').\n\nRestart any already-running agent session to pick them up.",
        root.display()
    )
}

/// Print the post-install summary to stdout.
///
/// Lists the destination paths for the installed skills and discloses that
/// skill names are prefixed with `baml-` to avoid registry collisions, so the
/// difference from upstream skill files is documented rather than silent.
///
/// # Parameters
/// - `root`: the project root the skills were installed under.
///
/// # Errors
/// Returns an error if writing to stdout fails.
fn print_success(root: &Path) -> Result<()> {
    writeln!(std::io::stdout(), "{}", success_message(root))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use super::*;

    #[test]
    fn direct_archive_layout_is_loaded() {
        let content = skill("baml-core");
        let archive = make_archive(&[("skills/baml-core/SKILL.md", content.as_str())]);
        let skills = skills_from_archive(&archive).unwrap().skills;

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "baml-core");
        assert!(skills[0].content.contains("name: baml-core"));
    }

    #[test]
    fn direct_archive_layout_installs_all_baml_skills() {
        let entries =
            direct_skill_entries(&["baml-core", "baml-bridges", "baml-serving", "baml-testing"]);
        let archive = make_archive(&entry_refs(&entries));

        let skills = skills_from_archive(&archive).unwrap().skills;
        let names = skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec!["baml-bridges", "baml-core", "baml-serving", "baml-testing"]
        );
    }

    #[test]
    fn archive_ignores_unrelated_entries_before_utf8_decode() {
        let content = skill("baml-core");
        let archive = make_archive_bytes(&[
            ("skills/baml-core/SKILL.md", content.as_bytes()),
            ("skills/baml-core/._SKILL.md", &[0xff, 0xfe]),
        ]);

        let skills = skills_from_archive(&archive).unwrap().skills;

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "baml-core");
    }

    #[test]
    fn legacy_plugin_layout_is_normalized() {
        let entries = ["core", "bridges", "testing"]
            .into_iter()
            .map(|legacy_name| {
                let content = format!(
                    "---\nname: {legacy_name}\ndescription: Use baml:testing, baml:bridges, and baml:*.\n---\n# {legacy_name}\n\nSee baml:testing and baml:bridges.\n"
                );
                (format!("plugins/baml/skills/{legacy_name}/SKILL.md"), content)
            })
            .collect::<Vec<_>>();
        let archive = make_archive(&entry_refs(&entries));

        let skills = skills_from_archive(&archive).unwrap().skills;
        let core = skills
            .iter()
            .find(|skill| skill.name == "baml-core")
            .unwrap();
        assert!(core.content.contains("name: baml-core"));
        assert!(!core.content.contains("baml:testing"));
        assert!(core.content.contains("baml-testing"));
        assert!(core.content.contains("baml-bridges"));
        assert!(core.content.contains("baml-*"));
    }

    #[test]
    fn direct_layout_requires_matching_frontmatter_name() {
        let archive = make_archive(&[("skills/baml-core/SKILL.md", skill("core").as_str())]);

        let err = format!("{:#}", skills_from_archive(&archive).unwrap_err());
        assert!(
            err.contains("frontmatter name must be `baml-core`"),
            "{err}"
        );
    }

    #[test]
    fn direct_layout_accepts_crlf_frontmatter_delimiters() {
        let content = "---\r\nname: baml-core\r\ndescription: test\r\n---\r\n# Core\r\n";
        let archive = make_archive(&[("skills/baml-core/SKILL.md", content)]);

        let skills = skills_from_archive(&archive).unwrap().skills;

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "baml-core");
    }

    #[test]
    fn install_refreshes_baml_skills_and_preserves_unrelated_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let stale = root.join(".agents/skills/baml-core/SKILL.md");
        fs::create_dir_all(stale.parent().unwrap()).unwrap();
        fs::write(&stale, "stale").unwrap();
        let unrelated = root.join(".agents/skills/other/SKILL.md");
        fs::create_dir_all(unrelated.parent().unwrap()).unwrap();
        fs::write(&unrelated, "keep").unwrap();

        let skills = ["baml-core", "baml-bridges"]
            .into_iter()
            .map(|name| Skill {
                name: name.to_string(),
                content: skill(name),
            })
            .collect::<Vec<_>>();

        install_skills(root, &skills).unwrap();

        assert!(
            fs::read_to_string(stale)
                .unwrap()
                .contains("name: baml-core")
        );
        assert_eq!(fs::read_to_string(unrelated).unwrap(), "keep");
        assert!(root.join(".claude/skills/baml-bridges/SKILL.md").is_file());
        assert!(
            fs::read_dir(root.join(".agents/skills"))
                .unwrap()
                .all(|entry| !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".baml-agent-install-backup-"))
        );
    }

    #[test]
    fn archive_pax_global_header_comment_becomes_commit() {
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let content = skill("baml-core");
        let archive =
            make_archive_with_pax(&[("skills/baml-core/SKILL.md", content.as_str())], Some(sha));

        let loaded = skills_from_archive(&archive).unwrap();

        assert_eq!(loaded.commit.as_deref(), Some(sha));
        assert_eq!(loaded.skills.len(), 1);
    }

    #[test]
    fn archive_without_pax_header_has_no_commit() {
        let content = skill("baml-core");
        let archive = make_archive(&[("skills/baml-core/SKILL.md", content.as_str())]);

        assert_eq!(skills_from_archive(&archive).unwrap().commit, None);
    }

    #[test]
    fn archive_with_non_sha_pax_comment_has_no_commit() {
        let content = skill("baml-core");
        let archive = make_archive_with_pax(
            &[("skills/baml-core/SKILL.md", content.as_str())],
            Some("not a commit sha"),
        );

        assert_eq!(skills_from_archive(&archive).unwrap().commit, None);
    }

    #[test]
    fn root_detection_prefers_nearest_baml_toml() {
        let _guard = cwd_lock().lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("project");
        let nested = root.join("a/b");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("baml.toml"), "[package]\nname = \"x\"\n").unwrap();

        let old = std::env::current_dir().unwrap();
        std::env::set_current_dir(&nested).unwrap();
        let detected = detect_install_root().unwrap();
        std::env::set_current_dir(old).unwrap();

        assert_eq!(detected, root.canonicalize().unwrap());
    }

    #[test]
    fn root_detection_accepts_baml_src_only_project() {
        let _guard = cwd_lock().lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("project");
        let nested = root.join("baml_src/nested");
        fs::create_dir_all(&nested).unwrap();

        let old = std::env::current_dir().unwrap();
        std::env::set_current_dir(&nested).unwrap();
        let detected = detect_install_root().unwrap();
        std::env::set_current_dir(old).unwrap();

        assert_eq!(detected, root.canonicalize().unwrap());
    }

    #[test]
    fn root_detection_falls_back_to_current_dir_outside_git() {
        let _guard = cwd_lock().lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let old = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let detected = detect_install_root().unwrap();
        std::env::set_current_dir(old).unwrap();

        assert_eq!(detected, tmp.path().canonicalize().unwrap());
    }

    #[test]
    fn explicit_dir_is_used_without_walk_up() {
        let _guard = cwd_lock().lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("project");
        let nested = project.join("nested");
        let explicit = tmp.path().join("custom");
        fs::create_dir_all(&nested).unwrap();
        fs::write(project.join("baml.toml"), "[package]\nname = \"x\"\n").unwrap();

        let old = std::env::current_dir().unwrap();
        std::env::set_current_dir(&nested).unwrap();
        let root = explicit_install_root(&explicit).unwrap();
        std::env::set_current_dir(old).unwrap();

        assert_eq!(root, explicit.canonicalize().unwrap());
    }

    #[test]
    fn success_message_discloses_baml_name_prefix() {
        let message = success_message(Path::new("/tmp/project"));
        assert!(
            message.contains(
                "skill names are prefixed with 'baml-' on install to avoid registry collisions"
            ),
            "{message}"
        );
        assert!(message.contains("Installed BAML agent skills in /tmp/project"));
    }

    fn skill(name: &str) -> String {
        format!(
            "---\nname: {name}\ndescription: Use when working with BAML and baml:testing.\n---\n# {name}\n\nSee baml:testing.\n"
        )
    }

    fn direct_skill_entries(names: &[&str]) -> Vec<(String, String)> {
        names
            .iter()
            .map(|name| (format!("skills/{name}/SKILL.md"), skill(name)))
            .collect()
    }

    fn entry_refs(entries: &[(String, String)]) -> Vec<(&str, &str)> {
        entries
            .iter()
            .map(|(path, content)| (path.as_str(), content.as_str()))
            .collect()
    }

    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn make_archive(entries: &[(&str, &str)]) -> Vec<u8> {
        let entries = entries
            .iter()
            .map(|(path, content)| (*path, content.as_bytes()))
            .collect::<Vec<_>>();
        make_archive_bytes(&entries)
    }

    /// Like [`make_archive`], but prepends a pax global header carrying
    /// `comment=<value>` the way `git archive` (and GitHub codeload) does.
    fn make_archive_with_pax(entries: &[(&str, &str)], pax_comment: Option<&str>) -> Vec<u8> {
        let mut archive_bytes = Vec::new();
        {
            let encoder =
                flate2::write::GzEncoder::new(&mut archive_bytes, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);
            if let Some(comment) = pax_comment {
                let record_content = format!("comment={comment}\n");
                let mut total = record_content.len();
                // The pax length prefix counts itself: grow until stable.
                loop {
                    let with_prefix = total.to_string().len() + 1 + record_content.len();
                    if with_prefix == total {
                        break;
                    }
                    total = with_prefix;
                }
                let record = format!("{total} {record_content}");
                let mut header = tar::Header::new_gnu();
                header.set_entry_type(tar::EntryType::XGlobalHeader);
                header.set_size(record.len() as u64);
                header.set_mode(0o666);
                header.set_cksum();
                builder
                    .append_data(&mut header, "pax_global_header", record.as_bytes())
                    .unwrap();
            }
            for (path, content) in entries.iter().copied() {
                let mut header = tar::Header::new_gnu();
                header.set_size(content.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder
                    .append_data(
                        &mut header,
                        format!("baml-skill-main/{path}"),
                        content.as_bytes(),
                    )
                    .unwrap();
            }
            builder.finish().unwrap();
        }
        archive_bytes
    }

    fn make_archive_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut archive_bytes = Vec::new();
        {
            let encoder =
                flate2::write::GzEncoder::new(&mut archive_bytes, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);
            for (path, content) in entries.iter().copied() {
                let mut header = tar::Header::new_gnu();
                header.set_size(content.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder
                    .append_data(&mut header, format!("baml-skill-main/{path}"), content)
                    .unwrap();
            }
            builder.finish().unwrap();
        }
        archive_bytes
    }
}
