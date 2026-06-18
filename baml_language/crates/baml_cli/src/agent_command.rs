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

/// Upstream `owner/repo` slug that hosts the canonical BAML agent skills.
const SKILL_REPO: &str = "BoundaryML/baml-skill";
/// Branch used to resolve and download skills for `--latest`.
const SKILL_BRANCH: &str = "main";
const SKILL_SOURCE_URL: &str =
    "https://codeload.github.com/BoundaryML/baml-skill/tar.gz/refs/heads/main";
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

    /// Install skills from the current BoundaryML/baml-skill main branch.
    #[arg(long, conflicts_with = "from")]
    pub latest: bool,

    /// Install skills from a tar.gz URL, local tar.gz archive, or local directory.
    #[arg(long, value_name = "URL_OR_PATH", conflicts_with = "latest")]
    pub from: Option<String>,

    /// Report what would change without writing any files (dry run).
    #[arg(long)]
    pub check: bool,
}

#[derive(Debug)]
struct Skill {
    name: String,
    content: String,
}

/// A resolved set of skills together with provenance for the install summary.
///
/// `reference` is a human-readable description of where the skills came from
/// (a branch, release tag, URL, or local path). `commit` is the upstream
/// commit SHA when it could be determined (only for `--latest`); it is `None`
/// otherwise.
#[derive(Debug)]
struct SkillSource {
    reference: String,
    commit: Option<String>,
    skills: Vec<Skill>,
}

/// A destination for installed skills, relative to the install root.
///
/// `label` is the agent family shown in the summary and `dir_display` is the
/// forward-slash relative directory (e.g. `.claude/skills`) used both for
/// display and to derive the on-disk path.
struct InstallTarget {
    label: &'static str,
    dir_display: &'static str,
}

/// The outcome of (dry-run) installing a skill into one target directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkillStatus {
    /// The skill directory did not exist and was (or would be) created.
    Created,
    /// The skill existed with different content and was (or would be) refreshed.
    Updated,
    /// The on-disk content already matched the source; nothing changed.
    Unchanged,
}

/// The per-skill statuses computed for a single install target.
#[derive(Debug)]
struct TargetReport {
    label: &'static str,
    dir_display: &'static str,
    statuses: Vec<(String, SkillStatus)>,
}

impl SkillStatus {
    /// Classifies the install status by comparing the desired `SKILL.md`
    /// content with what is currently on disk.
    ///
    /// `existing` is the current file content (`None` if the file is absent),
    /// and `desired` is the content that would be written. Returns
    /// [`SkillStatus::Created`] when absent, [`SkillStatus::Unchanged`] when
    /// byte-identical, and [`SkillStatus::Updated`] otherwise.
    fn classify(existing: Option<&str>, desired: &str) -> SkillStatus {
        match existing {
            None => SkillStatus::Created,
            Some(current) if current == desired => SkillStatus::Unchanged,
            Some(_) => SkillStatus::Updated,
        }
    }

    /// Returns the past-tense label shown after a real install.
    fn install_label(self) -> &'static str {
        match self {
            SkillStatus::Created => "created",
            SkillStatus::Updated => "updated",
            SkillStatus::Unchanged => "unchanged (already latest)",
        }
    }

    /// Returns the conditional label shown for a `--check` dry run.
    fn check_label(self) -> &'static str {
        match self {
            SkillStatus::Created => "would create",
            SkillStatus::Updated => "would update",
            SkillStatus::Unchanged => "up to date",
        }
    }

    /// Returns whether this status represents a change to the install tree.
    fn is_change(self) -> bool {
        !matches!(self, SkillStatus::Unchanged)
    }
}

/// Returns the ordered list of install targets (Claude Code first, then the
/// shared Codex / OpenCode directory). The order controls the summary layout.
fn install_targets() -> [InstallTarget; 2] {
    [
        InstallTarget {
            label: "Claude Code",
            dir_display: ".claude/skills",
        },
        InstallTarget {
            label: "Codex / OpenCode",
            dir_display: ".agents/skills",
        },
    ]
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
    /// Resolves the install root, loads the requested skills, then either
    /// installs them or (with `--check`) reports what would change, printing a
    /// per-skill, per-target status summary and the source ref/commit.
    ///
    /// Returns [`ExitCode::Success`] on success, or an error if the root cannot
    /// be resolved, the skills cannot be loaded, or a write fails.
    pub fn run(&self) -> Result<ExitCode> {
        let root = match &self.dir {
            Some(dir) => explicit_install_root(dir, !self.check)?,
            None => detect_install_root()?,
        };
        let source = load_skills(self)?;
        let reports = if self.check {
            check_skills(&root, &source.skills)?
        } else {
            install_skills(&root, &source.skills)?
        };
        print_report(&root, &source, &reports, self.check)?;
        Ok(ExitCode::Success)
    }
}

/// Loads the skills selected by `args` along with their provenance.
///
/// Dispatches to `--latest`, `--from <url-or-path>`, or the default versioned
/// release. Returns the resolved [`SkillSource`], or an error if the source
/// cannot be fetched, read, or parsed.
fn load_skills(args: &AgentInstallArgs) -> Result<SkillSource> {
    if args.latest {
        return load_latest_skills();
    }

    if let Some(source) = &args.from {
        return load_skills_from(source);
    }

    load_release_skills()
}

/// Loads skills from the `main` branch of `BoundaryML/baml-skill`.
///
/// Resolves the current commit SHA (best effort) and, when known, downloads
/// that exact commit so the reported commit matches the installed bytes.
/// Returns the [`SkillSource`], or an error if the archive cannot be fetched
/// or parsed.
fn load_latest_skills() -> Result<SkillSource> {
    let commit = resolve_latest_commit();
    let url = match commit.as_deref() {
        Some(sha) => format!("https://codeload.github.com/{SKILL_REPO}/tar.gz/{sha}"),
        None => SKILL_SOURCE_URL.to_string(),
    };
    let archive = fetch_url_bytes(&url)?;
    Ok(SkillSource {
        reference: format!("{SKILL_REPO}@{SKILL_BRANCH}"),
        commit,
        skills: skills_from_archive(&archive)?,
    })
}

/// Loads skills from a user-supplied `--from` source.
///
/// `source` may be an HTTP(S) tar.gz URL, a local directory, or a local
/// tar.gz archive. Returns the [`SkillSource`] with a `reference` describing
/// the origin and no commit, or an error if the source cannot be read.
fn load_skills_from(source: &str) -> Result<SkillSource> {
    if is_http_url(source) {
        let archive = fetch_url_bytes(source)?;
        return Ok(SkillSource {
            reference: source.to_string(),
            commit: None,
            skills: skills_from_archive(&archive)?,
        });
    }

    let path = Path::new(source);
    if path.is_dir() {
        return Ok(SkillSource {
            reference: format!("{} (local directory)", path.display()),
            commit: None,
            skills: skills_from_dir(path)?,
        });
    }

    let archive = fs::read(path).with_context(|| {
        format!(
            "failed to read BAML agent skills archive {}",
            path.display()
        )
    })?;
    Ok(SkillSource {
        reference: format!("{} (local archive)", path.display()),
        commit: None,
        skills: skills_from_archive(&archive)?,
    })
}

/// Loads skills from the versioned release asset matching this CLI build.
///
/// The version is taken from `BAML_AGENT_SKILLS_RELEASE_VERSION` or the
/// canonical build version, the archive checksum is verified, and the
/// [`SkillSource`] is returned. Errors propagate from fetching, checksum
/// verification, or parsing.
fn load_release_skills() -> Result<SkillSource> {
    let version = env_var_nonempty("BAML_AGENT_SKILLS_RELEASE_VERSION")
        .unwrap_or_else(|| baml_version::CANONICAL_VERSION.to_string());
    let url = versioned_skill_archive_url(&version);
    let archive = fetch_url_bytes(&url)?;
    let checksum_text = fetch_url_text(&format!("{url}.sha256"))?;
    baml_release::verify_release_archive_checksum_text(&archive, &url, &checksum_text)
        .context("verifying agent skills archive checksum failed")?;
    Ok(SkillSource {
        reference: format!("baml-language-{version} release"),
        commit: None,
        skills: skills_from_archive(&archive)?,
    })
}

/// Resolves the current commit SHA of the skills repo's `main` branch via the
/// GitHub API.
///
/// This is best effort: any network, HTTP, or JSON failure yields `None` so
/// installation still proceeds without a verified commit.
fn resolve_latest_commit() -> Option<String> {
    let url = format!("https://api.github.com/repos/{SKILL_REPO}/commits/{SKILL_BRANCH}");
    let text = fetch_url_text(&url).ok()?;
    let json = serde_json::from_str::<serde_json::Value>(&text).ok()?;
    json.get("sha")?.as_str().map(str::to_string)
}

fn is_http_url(value: &str) -> bool {
    value.starts_with("https://") || value.starts_with("http://")
}

fn env_var_nonempty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn versioned_skill_archive_url(version: &str) -> String {
    let base_url = env_var_nonempty("BAML_AGENT_SKILLS_RELEASE_BASE_URL");
    let repo = env_var_nonempty("BAML_AGENT_SKILLS_RELEASE_REPO")
        .unwrap_or_else(baml_release::release_repo);
    versioned_skill_archive_url_with_env(version, base_url.as_deref(), Some(&repo))
}

fn versioned_skill_archive_url_with_env(
    version: &str,
    base_url: Option<&str>,
    repo: Option<&str>,
) -> String {
    let filename = versioned_skill_archive_filename(version);
    if let Some(base_url) = base_url.map(str::trim).filter(|value| !value.is_empty()) {
        return format!("{}/{}", base_url.trim_end_matches('/'), filename);
    }

    let repo = repo
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(baml_release::DEFAULT_RELEASE_REPO);
    format!("https://github.com/{repo}/releases/download/baml-language-{version}/{filename}")
}

fn versioned_skill_archive_filename(version: &str) -> String {
    format!("baml-agent-skills-{version}.tar.gz")
}

fn fetch_url_text(url: &str) -> Result<String> {
    let bytes = fetch_url_bytes(url)?;
    String::from_utf8(bytes).with_context(|| format!("{url} was not valid UTF-8"))
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

/// Resolves the explicit `--dir` install root to an absolute path.
///
/// When `create` is true the directory is created if missing and then
/// canonicalized. When `create` is false (a `--check` dry run) a missing
/// directory is left untouched and resolved with [`std::path::absolute`] so
/// the dry run never writes to disk. Returns an error if creation or
/// resolution fails.
fn explicit_install_root(dir: &Path, create: bool) -> Result<PathBuf> {
    if create {
        fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    }
    match dir.canonicalize() {
        Ok(path) => Ok(path),
        Err(_) if !create && !dir.exists() => {
            std::path::absolute(dir).with_context(|| format!("failed to resolve {}", dir.display()))
        }
        Err(err) => Err(err).with_context(|| format!("failed to resolve {}", dir.display())),
    }
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
    let mut raw = Vec::new();

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
        let Some(skill_path) = skill_archive_path(&parts) else {
            continue;
        };

        let mut content = String::new();
        entry
            .read_to_string(&mut content)
            .with_context(|| format!("failed to read {}", path.display()))?;

        raw.push(raw_skill(skill_path, content, path));
    }

    normalize_skills(raw)
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

/// Installs `skills` into every target directory under `root`.
///
/// For each target the per-skill status is computed immediately before
/// writing. Only skills whose status represents a change ([`SkillStatus::Created`]
/// or [`SkillStatus::Updated`]) are written; skills reported as
/// [`SkillStatus::Unchanged`] are left untouched so their directories are not
/// needlessly rewritten or replaced. Returns one [`TargetReport`] per target in
/// display order, or an error if a status cannot be computed or any write fails.
fn install_skills(root: &Path, skills: &[Skill]) -> Result<Vec<TargetReport>> {
    let mut reports = Vec::new();
    for target in install_targets() {
        let statuses = target_statuses(root, &target, skills)?;
        let changed_skills = skills
            .iter()
            .zip(statuses.iter())
            .filter_map(|(skill, (_, status))| status.is_change().then_some(skill))
            .collect::<Vec<_>>();
        if !changed_skills.is_empty() {
            install_skills_to(root, Path::new(target.dir_display), &changed_skills)?;
        }
        reports.push(TargetReport {
            label: target.label,
            dir_display: target.dir_display,
            statuses,
        });
    }
    Ok(reports)
}

/// Computes the per-target, per-skill status without writing anything.
///
/// Used by `--check` to report what an install would do. Returns one
/// [`TargetReport`] per target in display order, or an error if any existing
/// install cannot be inspected.
fn check_skills(root: &Path, skills: &[Skill]) -> Result<Vec<TargetReport>> {
    install_targets()
        .iter()
        .map(|target| {
            Ok(TargetReport {
                label: target.label,
                dir_display: target.dir_display,
                statuses: target_statuses(root, target, skills)?,
            })
        })
        .collect()
}

/// Computes the [`SkillStatus`] for every skill in a single `target` without
/// modifying disk.
///
/// Reads each skill's existing `SKILL.md` and compares it against the source
/// content. A missing file (`NotFound`) is treated as absent, but any other I/O
/// failure (permission denied, invalid UTF-8, etc.) is propagated so the user
/// learns the existing install could not be inspected. Returns `(name, status)`
/// pairs in the same order as `skills`.
fn target_statuses(
    root: &Path,
    target: &InstallTarget,
    skills: &[Skill],
) -> Result<Vec<(String, SkillStatus)>> {
    let skills_dir = root.join(target.dir_display);
    skills
        .iter()
        .map(|skill| {
            let path = skills_dir.join(&skill.name).join("SKILL.md");
            let existing = match fs::read_to_string(&path) {
                Ok(content) => Some(content),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
                Err(err) => {
                    return Err(err).with_context(|| format!("failed to read {}", path.display()));
                }
            };
            let status = SkillStatus::classify(existing.as_deref(), &skill.content);
            Ok((skill.name.clone(), status))
        })
        .collect()
}

/// Installs every skill into `root/relative_skills_dir`, replacing any existing
/// `baml-*` skill directories atomically while preserving unrelated skills.
///
/// `relative_skills_dir` is the per-target relative directory (e.g.
/// `.claude/skills`). `skills` are the (already filtered) skills that should be
/// written; callers typically pass only skills whose status represents a change.
/// Returns an error if any directory or file operation fails; partial installs
/// are rolled back per skill where possible.
fn install_skills_to(root: &Path, relative_skills_dir: &Path, skills: &[&Skill]) -> Result<()> {
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

/// Renders and writes the install (or dry-run) summary to stdout.
///
/// Delegates to [`render_report`] and writes the result. Returns an error if
/// stdout cannot be written.
fn print_report(
    root: &Path,
    source: &SkillSource,
    reports: &[TargetReport],
    check: bool,
) -> Result<()> {
    let text = render_report(root, source, reports, check);
    write!(std::io::stdout(), "{text}")?;
    Ok(())
}

/// Builds the install (or dry-run) summary string.
///
/// The summary lists the install root, the source `reference` and resolved
/// `commit` (when known), a per-skill status line under each target, and a
/// trailing reminder (real install) or freshness verdict (`--check`).
///
/// Successful install summaries also disclose the `baml-` skill name-prefix
/// mapping so users can reconcile installed names with upstream names. When
/// `check` is true, conditional ("would create") labels are used and no files
/// are written by the caller. Returns the rendered text.
fn render_report(
    root: &Path,
    source: &SkillSource,
    reports: &[TargetReport],
    check: bool,
) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    let header = if check {
        "Checked BAML agent skills in"
    } else {
        "Installed BAML agent skills in"
    };
    let _ = writeln!(out, "{header} {}", root.display());
    match &source.commit {
        Some(commit) => {
            let _ = writeln!(out, "Source: {} (commit {commit})", source.reference);
        }
        None => {
            let _ = writeln!(out, "Source: {}", source.reference);
        }
    }

    for report in reports {
        let _ = writeln!(out, "\n{} ({}):", report.label, report.dir_display);
        for (name, status) in &report.statuses {
            let label = if check {
                status.check_label()
            } else {
                status.install_label()
            };
            let _ = writeln!(out, "  {name}: {label}");
        }
    }

    out.push('\n');
    if check {
        let changes = reports
            .iter()
            .flat_map(|report| &report.statuses)
            .filter(|(_, status)| status.is_change())
            .count();
        if changes == 0 {
            let _ = writeln!(out, "All BAML agent skills are already up to date.");
        } else {
            let _ = writeln!(
                out,
                "{changes} skill installation(s) are out of date; run `baml agent install` to apply them."
            );
        }
    } else {
        let _ = writeln!(
            out,
            "Note: skill names are prefixed with 'baml-' on install to avoid registry collisions (e.g. upstream 'core' becomes 'baml-core')."
        );
        out.push('\n');
        let _ = writeln!(
            out,
            "Restart any already-running agent session to pick them up."
        );
    }

    out
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
        assert!(skills[0].content.contains("name: baml-core"));
    }

    #[test]
    fn direct_archive_layout_installs_all_baml_skills() {
        let entries =
            direct_skill_entries(&["baml-core", "baml-bridges", "baml-serving", "baml-testing"]);
        let archive = make_archive(&entry_refs(&entries));

        let skills = skills_from_archive(&archive).unwrap();
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

        let skills = skills_from_archive(&archive).unwrap();

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

        let skills = skills_from_archive(&archive).unwrap();
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

        let skills = skills_from_archive(&archive).unwrap();

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
    fn versioned_archive_url_defaults_to_release_asset() {
        assert_eq!(
            versioned_skill_archive_url_with_env("1.2.3", None, None),
            "https://github.com/BoundaryML/baml/releases/download/baml-language-1.2.3/baml-agent-skills-1.2.3.tar.gz"
        );
        assert_eq!(
            versioned_skill_archive_url_with_env("1.2.3", Some("https://example.com/base/"), None),
            "https://example.com/base/baml-agent-skills-1.2.3.tar.gz"
        );
        assert_eq!(
            versioned_skill_archive_url_with_env("1.2.3", None, Some("BoundaryML/custom")),
            "https://github.com/BoundaryML/custom/releases/download/baml-language-1.2.3/baml-agent-skills-1.2.3.tar.gz"
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
        let root = explicit_install_root(&explicit, true).unwrap();
        std::env::set_current_dir(old).unwrap();

        assert_eq!(root, explicit.canonicalize().unwrap());
    }

    #[test]
    fn check_mode_does_not_create_missing_explicit_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist");

        let root = explicit_install_root(&missing, false).unwrap();

        assert_eq!(root, missing);
        assert!(!missing.exists());
    }

    #[test]
    fn skill_status_classifies_created_updated_and_unchanged() {
        assert_eq!(SkillStatus::classify(None, "x"), SkillStatus::Created);
        assert_eq!(
            SkillStatus::classify(Some("x"), "x"),
            SkillStatus::Unchanged
        );
        assert_eq!(
            SkillStatus::classify(Some("old"), "new"),
            SkillStatus::Updated
        );
    }

    #[test]
    fn target_statuses_reflect_disk_state() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let target = &install_targets()[0];
        let existing = root
            .join(target.dir_display)
            .join("baml-core")
            .join("SKILL.md");
        fs::create_dir_all(existing.parent().unwrap()).unwrap();
        fs::write(&existing, skill("baml-core")).unwrap();

        let skills = vec![
            Skill {
                name: "baml-core".to_string(),
                content: skill("baml-core"),
            },
            Skill {
                name: "baml-overview".to_string(),
                content: skill("baml-overview"),
            },
        ];

        let statuses = target_statuses(root, target, &skills).unwrap();

        assert_eq!(
            statuses,
            vec![
                ("baml-core".to_string(), SkillStatus::Unchanged),
                ("baml-overview".to_string(), SkillStatus::Created),
            ]
        );
    }

    #[test]
    fn check_skills_does_not_write_to_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let skills = vec![Skill {
            name: "baml-core".to_string(),
            content: skill("baml-core"),
        }];

        let reports = check_skills(root, &skills).unwrap();

        assert_eq!(reports.len(), 2);
        for report in &reports {
            assert_eq!(
                report.statuses,
                vec![("baml-core".to_string(), SkillStatus::Created)]
            );
        }
        assert!(!root.join(".claude").exists());
        assert!(!root.join(".agents").exists());
    }

    #[test]
    fn install_then_reinstall_reports_created_then_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let skills = vec![Skill {
            name: "baml-core".to_string(),
            content: skill("baml-core"),
        }];

        let first = install_skills(root, &skills).unwrap();
        assert!(
            first
                .iter()
                .flat_map(|report| &report.statuses)
                .all(|(_, status)| *status == SkillStatus::Created)
        );

        let second = install_skills(root, &skills).unwrap();
        assert!(
            second
                .iter()
                .flat_map(|report| &report.statuses)
                .all(|(_, status)| *status == SkillStatus::Unchanged)
        );
    }

    #[test]
    fn reinstall_does_not_rewrite_unchanged_skill_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let skills = vec![Skill {
            name: "baml-core".to_string(),
            content: skill("baml-core"),
        }];

        install_skills(root, &skills).unwrap();

        // Drop a sidecar file inside an installed skill directory. A reinstall
        // that classifies the skill as `Unchanged` must leave the directory
        // (and this file) untouched rather than replacing it wholesale.
        let sidecar = root.join(".claude/skills/baml-core/extra.txt");
        fs::write(&sidecar, "keep me").unwrap();

        let report = install_skills(root, &skills).unwrap();
        assert!(
            report
                .iter()
                .flat_map(|report| &report.statuses)
                .all(|(_, status)| *status == SkillStatus::Unchanged)
        );
        assert_eq!(fs::read_to_string(&sidecar).unwrap(), "keep me");
    }

    #[test]
    fn render_report_shows_commit_and_statuses() {
        let source = SkillSource {
            reference: format!("{SKILL_REPO}@{SKILL_BRANCH}"),
            commit: Some("0baf1692be0ff85bbe3fc3ecabe84b00a010b020".to_string()),
            skills: Vec::new(),
        };
        let reports = vec![TargetReport {
            label: "Claude Code",
            dir_display: ".claude/skills",
            statuses: vec![
                ("baml-core".to_string(), SkillStatus::Unchanged),
                ("baml-overview".to_string(), SkillStatus::Updated),
            ],
        }];

        let install = render_report(Path::new("/tmp/staging"), &source, &reports, false);
        assert!(install.contains("Installed BAML agent skills in /tmp/staging"));
        assert!(install.contains(
            "Source: BoundaryML/baml-skill@main (commit 0baf1692be0ff85bbe3fc3ecabe84b00a010b020)"
        ));
        assert!(install.contains("Claude Code (.claude/skills):"));
        assert!(install.contains("baml-core: unchanged (already latest)"));
        assert!(install.contains("baml-overview: updated"));
        assert!(install.contains("Restart any already-running agent session"));

        let check = render_report(Path::new("/tmp/staging"), &source, &reports, true);
        assert!(check.contains("Checked BAML agent skills in /tmp/staging"));
        assert!(check.contains("baml-core: up to date"));
        assert!(check.contains("baml-overview: would update"));
        assert!(check.contains("out of date"));
    }

    #[test]
    fn render_report_without_commit_omits_commit_suffix() {
        let source = SkillSource {
            reference: "/tmp/skillsrc (local directory)".to_string(),
            commit: None,
            skills: Vec::new(),
        };
        let reports = vec![TargetReport {
            label: "Claude Code",
            dir_display: ".claude/skills",
            statuses: vec![("baml-core".to_string(), SkillStatus::Unchanged)],
        }];

        let check = render_report(Path::new("/tmp/staging"), &source, &reports, true);
        assert!(check.contains("Source: /tmp/skillsrc (local directory)"));
        assert!(!check.contains("commit"));
        assert!(check.contains("All BAML agent skills are already up to date."));
    }

    #[test]
    fn render_report_discloses_baml_name_prefix_on_install() {
        let source = SkillSource {
            reference: format!("{SKILL_REPO}@{SKILL_BRANCH}"),
            commit: Some("0baf1692be0ff85bbe3fc3ecabe84b00a010b020".to_string()),
            skills: Vec::new(),
        };
        let reports = vec![TargetReport {
            label: "Claude Code",
            dir_display: ".claude/skills",
            statuses: vec![("baml-core".to_string(), SkillStatus::Created)],
        }];
        let install = render_report(Path::new("/tmp/project"), &source, &reports, false);
        assert!(
            install.contains(
                "skill names are prefixed with 'baml-' on install to avoid registry collisions"
            ),
            "{install}"
        );
        assert!(install.contains("Installed BAML agent skills in /tmp/project"));
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
