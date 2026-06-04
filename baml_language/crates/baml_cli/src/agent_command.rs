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

use crate::ExitCode;

const SKILL_SOURCE_URL: &str =
    "https://codeload.github.com/BoundaryML/baml-skill/tar.gz/refs/heads/main";
const HTTP_TIMEOUT: Duration = Duration::from_secs(60);

const SKILLS: &[SkillSpec] = &[
    SkillSpec {
        direct_name: "baml-core",
        legacy_name: "core",
    },
    SkillSpec {
        direct_name: "baml-llm-functions",
        legacy_name: "llm-functions",
    },
    SkillSpec {
        direct_name: "baml-pipelines",
        legacy_name: "pipelines",
    },
    SkillSpec {
        direct_name: "baml-testing",
        legacy_name: "testing",
    },
    SkillSpec {
        direct_name: "baml-bridges",
        legacy_name: "bridges",
    },
];

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
}

#[derive(Clone, Copy)]
struct SkillSpec {
    direct_name: &'static str,
    legacy_name: &'static str,
}

#[derive(Debug)]
struct Skill {
    name: &'static str,
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
        let archive = fetch_latest_skill_archive()?;
        let skills = skills_from_archive(&archive)?;
        install_skills(&root, &skills)?;
        print_success(&root)?;
        Ok(ExitCode::Success)
    }
}

fn fetch_latest_skill_archive() -> Result<Vec<u8>> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(HTTP_TIMEOUT)
        .user_agent("baml-agent-install/1")
        .build()
        .context("failed to build HTTP client")?;

    let response = client
        .get(SKILL_SOURCE_URL)
        .send()
        .with_context(|| format!("failed to fetch {SKILL_SOURCE_URL}"))?;
    if !response.status().is_success() {
        anyhow::bail!(
            "failed to fetch BAML agent skills from {SKILL_SOURCE_URL}: HTTP {}",
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
    let mut found = BTreeMap::<&'static str, String>::new();

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
        let Some(spec) = skill_spec_for_archive_path(&parts) else {
            continue;
        };

        let mut content = String::new();
        entry
            .read_to_string(&mut content)
            .with_context(|| format!("failed to read {}", path.display()))?;

        let normalized = if archive_path_is_direct_skill(&parts, spec.direct_name) {
            validate_skill_name(&content, spec.direct_name)?;
            normalize_direct_skill_references(content, spec)
        } else {
            normalize_legacy_skill_content(&content, spec)?
        };
        found.insert(spec.direct_name, normalized);
    }

    let mut skills = Vec::new();
    for spec in SKILLS {
        let Some(content) = found.remove(spec.direct_name) else {
            anyhow::bail!(
                "BAML agent skills archive is missing skills/{}/SKILL.md",
                spec.direct_name
            );
        };
        skills.push(Skill {
            name: spec.direct_name,
            content,
        });
    }
    Ok(skills)
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

fn skill_spec_for_archive_path(parts: &[String]) -> Option<SkillSpec> {
    for spec in SKILLS {
        if archive_path_is_direct_skill(parts, spec.direct_name)
            || archive_path_is_legacy_plugin_skill(parts, spec.legacy_name)
        {
            return Some(*spec);
        }
    }
    None
}

fn archive_path_is_direct_skill(parts: &[String], skill_name: &str) -> bool {
    parts.len() >= 3
        && parts[parts.len() - 3] == "skills"
        && parts[parts.len() - 2] == skill_name
        && parts[parts.len() - 1] == "SKILL.md"
}

fn archive_path_is_legacy_plugin_skill(parts: &[String], skill_name: &str) -> bool {
    parts.len() >= 5
        && parts[parts.len() - 5] == "plugins"
        && parts[parts.len() - 4] == "baml"
        && parts[parts.len() - 3] == "skills"
        && parts[parts.len() - 2] == skill_name
        && parts[parts.len() - 1] == "SKILL.md"
}

fn normalize_legacy_skill_content(content: &str, spec: SkillSpec) -> Result<String> {
    let (frontmatter, body) = split_frontmatter(content)?;
    let frontmatter = replace_frontmatter_name(frontmatter, spec.direct_name)?;
    let content = format!("---\n{frontmatter}---\n{body}");
    Ok(normalize_direct_skill_references(content, spec))
}

fn normalize_direct_skill_references(mut content: String, _spec: SkillSpec) -> String {
    for skill in SKILLS {
        content = content.replace(&format!("baml:{}", skill.legacy_name), skill.direct_name);
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
            let skill_dir = tmp_dir.join(skill.name);
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
    let final_dir = skills_dir.join(skill.name);
    let next_dir = tmp_dir.join(skill.name);
    let backup_dir = unique_backup_dir(skills_dir, skill.name)?;
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
        "Installed latest BAML agent skills in {}\n\nClaude Code:\n  .claude/skills/baml-*/SKILL.md\n\nCodex / OpenCode:\n  .agents/skills/baml-*/SKILL.md\n\nRestart any already-running agent session to pick them up.",
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
        let err = skills_from_archive(&archive).unwrap_err().to_string();
        assert!(err.contains("baml-llm-functions"), "{err}");
    }

    #[test]
    fn legacy_plugin_layout_is_normalized() {
        let entries = SKILLS
            .iter()
            .map(|spec| {
                (
                    format!("plugins/baml/skills/{}/SKILL.md", spec.legacy_name),
                    skill(spec.legacy_name),
                )
            })
            .collect::<Vec<_>>();
        let entries = entries
            .iter()
            .map(|(path, content)| (path.as_str(), content.as_str()))
            .collect::<Vec<_>>();
        let archive = make_archive(&entries);

        let skills = skills_from_archive(&archive).unwrap();
        let core = skills
            .iter()
            .find(|skill| skill.name == "baml-core")
            .unwrap();
        assert!(core.content.contains("name: baml-core"));
        assert!(!core.content.contains("baml:testing"));
    }

    #[test]
    fn direct_layout_requires_matching_frontmatter_name() {
        let entries = SKILLS
            .iter()
            .map(|spec| {
                (
                    format!("skills/{}/SKILL.md", spec.direct_name),
                    skill(spec.legacy_name),
                )
            })
            .collect::<Vec<_>>();
        let entries = entries
            .iter()
            .map(|(path, content)| (path.as_str(), content.as_str()))
            .collect::<Vec<_>>();
        let archive = make_archive(&entries);

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

        let skills = SKILLS
            .iter()
            .map(|spec| Skill {
                name: spec.direct_name,
                content: skill(spec.direct_name),
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
