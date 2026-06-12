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

use crate::{ExitCode, commands::release_version};

const LATEST_SKILL_SOURCE_URL: &str =
    "https://codeload.github.com/BoundaryML/baml-skill/tar.gz/refs/heads/main";
const HTTP_TIMEOUT: Duration = Duration::from_secs(60);
const AGENT_SKILLS_ARTIFACT_PREFIX: &str = "baml-agent-skills";

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

#[derive(Args, Clone, Debug)]
pub(crate) struct AgentInstallArgs {
    /// Directory where project-local agent skills should be installed.
    ///
    /// When omitted, BAML installs at the nearest ancestor with baml.toml,
    /// then the git root, then the current directory.
    #[arg(long, value_name = "PATH")]
    pub dir: Option<PathBuf>,

    /// Install the latest skills from BoundaryML/baml-skill main instead of the
    /// version pinned to this BAML CLI.
    #[arg(long, conflicts_with = "from")]
    pub latest: bool,

    /// Install skills from a tar.gz archive URL, local archive path, or local
    /// directory. Intended for testing and emergency hotfixes.
    #[arg(long, value_name = "URL_OR_PATH", conflicts_with = "latest")]
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
    legacy_name: String,
    content: String,
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
        let skills = self.load_skills()?;
        install_skills(&root, &skills)?;
        print_success(&root)?;
        Ok(ExitCode::Success)
    }
}

impl AgentInstallArgs {
    fn load_skills(&self) -> Result<Vec<Skill>> {
        if self.latest {
            let archive = fetch_archive_url(LATEST_SKILL_SOURCE_URL)?;
            return skills_from_archive(&archive);
        }

        if let Some(source) = &self.from {
            return skills_from_source(source);
        }

        let archive = fetch_versioned_skill_archive()?;
        skills_from_archive(&archive)
    }
}

fn fetch_versioned_skill_archive() -> Result<Vec<u8>> {
    let version = agent_skills_release_version();
    let url = versioned_skill_archive_url(&version);
    let archive = fetch_archive_url(&url)?;
    let checksum_url = format!("{url}.sha256");
    let checksum = fetch_archive_url(&checksum_url)?;
    let checksum_text =
        std::str::from_utf8(&checksum).context("BAML agent skills checksum was not valid UTF-8")?;
    baml_release::verify_release_archive_checksum_text(&archive, &url, checksum_text)
        .context("verifying agent skills archive checksum failed")?;
    Ok(archive)
}

fn agent_skills_release_version() -> String {
    std::env::var("BAML_AGENT_SKILLS_RELEASE_VERSION")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| release_version().to_string())
}

fn versioned_skill_archive_url(version: &str) -> String {
    let base_url = std::env::var("BAML_AGENT_SKILLS_RELEASE_BASE_URL").ok();
    let repo = std::env::var("BAML_AGENT_SKILLS_RELEASE_REPO").ok();
    versioned_skill_archive_url_with_env(version, base_url.as_deref(), repo.as_deref())
}

fn versioned_skill_archive_url_with_env(
    version: &str,
    base_url: Option<&str>,
    repo: Option<&str>,
) -> String {
    let filename = versioned_skill_archive_filename(version);
    if let Some(base_url) = base_url {
        let base = base_url.trim_end_matches('/');
        return format!("{base}/{filename}");
    }

    let repo = repo
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(baml_release::release_repo);
    format!("https://github.com/{repo}/releases/download/baml-language-{version}/{filename}")
}

fn versioned_skill_archive_filename(version: &str) -> String {
    format!("{AGENT_SKILLS_ARTIFACT_PREFIX}-{version}.tar.gz")
}

fn skills_from_source(source: &str) -> Result<Vec<Skill>> {
    if source.starts_with("https://") || source.starts_with("http://") {
        let archive = fetch_archive_url(source)?;
        return skills_from_archive(&archive);
    }

    let path = Path::new(source);
    if path.is_dir() {
        return skills_from_directory(path);
    }

    let archive = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    skills_from_archive(&archive)
}

fn fetch_archive_url(url: &str) -> Result<Vec<u8>> {
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

    if let Some(root) = canonical
        .ancestors()
        .find(|dir| dir.join("baml.toml").is_file())
    {
        return Ok(root.to_path_buf());
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

fn skills_from_archive(archive: &[u8]) -> Result<Vec<Skill>> {
    let decoder = flate2::read::GzDecoder::new(Cursor::new(archive));
    let mut archive = tar::Archive::new(decoder);
    let mut files = BTreeMap::<String, String>::new();

    for entry in archive.entries().context("failed to read skill archive")? {
        let mut entry = entry.context("failed to read skill archive entry")?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry
            .path()
            .context("failed to read skill archive entry path")?
            .into_owned();
        let parts = normalized_components(&path)?;
        if !archive_entry_might_be_agent_skill_file(&parts) {
            continue;
        }
        let mut content = String::new();
        entry
            .read_to_string(&mut content)
            .with_context(|| format!("failed to read {}", path.display()))?;
        files.insert(parts.join("/"), content);
    }

    skills_from_files(&files)
}

fn archive_entry_might_be_agent_skill_file(parts: &[String]) -> bool {
    matches!(parts.last().map(String::as_str), Some("SKILL.md"))
}

fn skills_from_directory(root: &Path) -> Result<Vec<Skill>> {
    let mut files = BTreeMap::<String, String>::new();
    collect_skill_files_from_directory(root, root, &mut files)?;
    skills_from_files(&files)
}

fn collect_skill_files_from_directory(
    root: &Path,
    dir: &Path,
    files: &mut BTreeMap<String, String>,
) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry.with_context(|| format!("failed to read {}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", path.display()))?;
        if file_type.is_dir() {
            collect_skill_files_from_directory(root, &path, files)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .with_context(|| format!("failed to relativize {}", path.display()))?;
            let parts = normalized_components(relative)?;
            if parts.last().map(String::as_str) == Some("SKILL.md") {
                let content = fs::read_to_string(&path)
                    .with_context(|| format!("failed to read {}", path.display()))?;
                files.insert(parts.join("/"), content);
            }
        }
    }

    Ok(())
}

fn skills_from_files(files: &BTreeMap<String, String>) -> Result<Vec<Skill>> {
    let raw = raw_skills_from_discovered_files(files)?;
    normalize_raw_skills(raw)
}

fn raw_skills_from_discovered_files(files: &BTreeMap<String, String>) -> Result<Vec<RawSkill>> {
    let mut raw = BTreeMap::<String, RawSkill>::new();

    for (path, content) in files {
        let parts = path_components(path);
        if let Some(name) = direct_skill_name_from_parts(&parts) {
            validate_skill_install_name(name)?;
            validate_skill_name(content, name)?;
            raw.insert(
                name.to_string(),
                RawSkill {
                    name: name.to_string(),
                    legacy_name: legacy_name_for_direct_skill(name)?,
                    content: content.clone(),
                },
            );
        } else if let Some(legacy_name) = legacy_plugin_skill_name_from_parts(&parts) {
            let name = format!("baml-{legacy_name}");
            validate_skill_install_name(&name)?;
            raw.insert(
                name.clone(),
                RawSkill {
                    name: name.clone(),
                    legacy_name: legacy_name.to_string(),
                    content: rewrite_frontmatter_name(content, &name)?,
                },
            );
        }
    }

    if raw.is_empty() {
        anyhow::bail!(
            "BAML agent skills archive does not contain any skills. Expected skills/baml-*/SKILL.md or plugins/baml/skills/*/SKILL.md"
        );
    }

    Ok(raw.into_values().collect())
}

fn normalize_raw_skills(raw: Vec<RawSkill>) -> Result<Vec<Skill>> {
    let mut legacy_to_direct = BTreeMap::new();
    for skill in &raw {
        if legacy_to_direct
            .insert(skill.legacy_name.clone(), skill.name.clone())
            .is_some()
        {
            anyhow::bail!("duplicate BAML agent skill {}", skill.legacy_name);
        }
    }

    Ok(raw
        .into_iter()
        .map(|skill| Skill {
            name: skill.name,
            content: normalize_skill_references(skill.content, &legacy_to_direct),
        })
        .collect())
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

fn path_components(path: &str) -> Vec<&str> {
    path.split('/').filter(|part| !part.is_empty()).collect()
}

fn direct_skill_name_from_parts<'a>(parts: &'a [&str]) -> Option<&'a str> {
    (parts.len() >= 3
        && parts[parts.len() - 3] == "skills"
        && parts[parts.len() - 2].starts_with("baml-")
        && parts[parts.len() - 1] == "SKILL.md")
        .then_some(parts[parts.len() - 2])
}

fn legacy_plugin_skill_name_from_parts<'a>(parts: &'a [&str]) -> Option<&'a str> {
    (parts.len() >= 5
        && parts[parts.len() - 5] == "plugins"
        && parts[parts.len() - 4] == "baml"
        && parts[parts.len() - 3] == "skills"
        && parts[parts.len() - 1] == "SKILL.md")
        .then_some(parts[parts.len() - 2])
}

fn rewrite_frontmatter_name(content: &str, name: &str) -> Result<String> {
    let (frontmatter, body) = split_frontmatter(content)?;
    let frontmatter = replace_frontmatter_name(frontmatter, name)?;
    Ok(format!("---\n{frontmatter}---\n{body}"))
}

fn normalize_skill_references(
    mut content: String,
    legacy_to_direct: &BTreeMap<String, String>,
) -> String {
    for (legacy_name, direct_name) in legacy_to_direct {
        content = content.replace(&format!("baml:{legacy_name}"), direct_name);
    }
    content = content.replace("baml:*", "baml-*");
    content
}

fn split_frontmatter(content: &str) -> Result<(&str, &str)> {
    let rest = content
        .strip_prefix("---\n")
        .ok_or_else(|| anyhow!("SKILL.md is missing opening frontmatter marker"))?;
    let Some((frontmatter, body)) = rest.split_once("\n---\n") else {
        anyhow::bail!("SKILL.md is missing closing frontmatter marker");
    };
    Ok((frontmatter, body))
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

fn validate_skill_install_name(name: &str) -> Result<()> {
    if !name.starts_with("baml-")
        || name.len() <= "baml-".len()
        || !name
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
    {
        anyhow::bail!("BAML agent skill name must look like `baml-name`, got `{name}`");
    }
    Ok(())
}

fn legacy_name_for_direct_skill(name: &str) -> Result<String> {
    validate_skill_install_name(name)?;
    Ok(name.trim_start_matches("baml-").to_string())
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

fn print_success(root: &Path) -> Result<()> {
    writeln!(
        std::io::stdout(),
        "Installed BAML agent skills in {}\n\nClaude Code:\n  .claude/skills/baml-*/SKILL.md\n\nCodex / OpenCode:\n  .agents/skills/baml-*/SKILL.md\n\nRestart any already-running agent session to pick them up.",
        root.display()
    )?;
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
        let skills = skills_from_archive(&archive).unwrap();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "baml-core");
    }

    #[test]
    fn legacy_plugin_layout_is_normalized() {
        let entries = ["core", "bridges", "serving", "testing"]
            .into_iter()
            .map(|name| (format!("plugins/baml/skills/{name}/SKILL.md"), skill(name)))
            .collect::<Vec<_>>();
        let entries = entries
            .iter()
            .map(|(path, content)| (path.as_str(), content.as_str()))
            .collect::<Vec<_>>();
        let archive = make_archive(&entries);

        let skills = skills_from_archive(&archive).unwrap();
        assert_eq!(
            skills
                .iter()
                .map(|skill| skill.name.as_str())
                .collect::<Vec<_>>(),
            vec!["baml-bridges", "baml-core", "baml-serving", "baml-testing"]
        );
        let core = skills
            .iter()
            .find(|skill| skill.name == "baml-core")
            .unwrap();
        assert!(core.content.contains("name: baml-core"));
        assert!(!core.content.contains("baml:testing"));
        assert!(core.content.contains("baml-testing"));
    }

    #[test]
    fn direct_layout_installs_all_baml_skills() {
        let core = skill("baml-core");
        let serving = skill("baml-serving");
        let ignored = skill("baml-ignored");
        let archive = make_archive(&[
            ("skills/baml-core/SKILL.md", core.as_str()),
            ("skills/baml-serving/SKILL.md", serving.as_str()),
            ("skills/baml-ignored/SKILL.md", ignored.as_str()),
        ]);

        let skills = skills_from_archive(&archive).unwrap();

        assert_eq!(
            skills
                .iter()
                .map(|skill| skill.name.as_str())
                .collect::<Vec<_>>(),
            vec!["baml-core", "baml-ignored", "baml-serving"]
        );
    }

    #[test]
    fn direct_layout_requires_matching_frontmatter_name() {
        let content = skill("core");
        let archive = make_archive(&[("skills/baml-core/SKILL.md", content.as_str())]);

        let err = skills_from_archive(&archive).unwrap_err().to_string();
        assert!(
            err.contains("frontmatter name must be `baml-core`"),
            "{err}"
        );
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

        let skills = ["baml-core", "baml-serving"]
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
        assert!(root.join(".claude/skills/baml-serving/SKILL.md").is_file());
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
    fn versioned_archive_url_uses_cli_release_tag() {
        assert_eq!(
            versioned_skill_archive_url_with_env("1.2.3-nightly.20260612.a", None, None),
            "https://github.com/BoundaryML/baml/releases/download/baml-language-1.2.3-nightly.20260612.a/baml-agent-skills-1.2.3-nightly.20260612.a.tar.gz"
        );
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

    fn skill(name: &str) -> String {
        format!(
            "---\nname: {name}\ndescription: Use when working with BAML and baml:testing.\n---\n# {name}\n\nSee baml:testing.\n"
        )
    }

    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn make_archive(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut archive_bytes = Vec::new();
        {
            let encoder =
                flate2::write::GzEncoder::new(&mut archive_bytes, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);
            for (path, content) in entries {
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
}
