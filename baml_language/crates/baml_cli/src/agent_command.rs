use std::{
    fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use clap::Args;

use crate::ExitCode;

const SKILL_NAME: &str = "baml";
const LEGACY_SKILL_NAME: &str = "baml-core";
const OLD_SKILLS_DIR: &str = "baml-old_skills";
const BOOTSTRAP: BootstrapSpec = BootstrapSpec {
    version: 1,
    contents: include_str!("../../../../skill/bootstrap.md"),
};
const MAIN_GUIDE: &str = include_str!("../../../../skill/guides/main.md");

#[derive(Clone, Copy, Debug)]
struct BootstrapSpec {
    version: u32,
    contents: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InstallAction {
    Created,
    Updated,
    Unchanged,
}

#[derive(Debug, Eq, PartialEq)]
struct TargetReport {
    path: PathBuf,
    action: InstallAction,
    archived: Vec<PathBuf>,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct InstallReport {
    targets: Vec<TargetReport>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VersionRelation {
    Unknown,
    Current,
    BootstrapOutdated,
    ToolchainOutdated,
}

#[derive(Args, Clone, Debug)]
pub(crate) struct AgentArgs {
    #[command(subcommand)]
    pub command: AgentCommand,
}

#[derive(clap::Subcommand, Clone, Debug)]
pub(crate) enum AgentCommand {
    #[command(about = "Print the agent guide bundled with this BAML toolchain")]
    Guide(AgentGuideArgs),

    #[command(about = "Install or refresh the BAML agent bootstrap in this project")]
    Install(AgentInstallArgs),
}

#[derive(Args, Clone, Debug)]
#[command(after_long_help = "\
Examples:
  Print the default guide:
    baml agent guide

  Print the main guide explicitly:
    baml agent guide main")]
pub(crate) struct AgentGuideArgs {
    /// Guide to print.
    #[arg(value_name = "GUIDE", default_value = "main")]
    pub guide: String,

    /// Bootstrap protocol version reported by the installed skill.
    #[arg(long, value_name = "VERSION", hide = true)]
    pub bootstrap_version: Option<u32>,
}

#[derive(Args, Clone, Debug)]
#[command(after_long_help = "\
Examples:
  Install the BAML agent bootstrap:
    baml agent install

  Install the bootstrap in a specific project:
    baml agent install --project ./my-project")]
pub(crate) struct AgentInstallArgs {
    /// Deprecated alias for `--project`.
    #[arg(long, value_name = "PATH", hide = true)]
    pub dir: Option<PathBuf>,
}

impl AgentArgs {
    pub fn run(&self) -> Result<ExitCode> {
        match &self.command {
            AgentCommand::Guide(args) => args.run(),
            AgentCommand::Install(args) => args.run(),
        }
    }
}

impl AgentGuideArgs {
    fn run(&self) -> Result<ExitCode> {
        let guide = match self.guide.as_str() {
            "main" => MAIN_GUIDE,
            name => anyhow::bail!("unknown agent guide `{name}`; available guides: main"),
        };
        if let Some(warning) =
            bootstrap_warning(version_relation(self.bootstrap_version, BOOTSTRAP.version))
        {
            write_stdout(warning)?;
        }
        write_stdout(guide)?;
        Ok(ExitCode::Success)
    }
}

impl AgentInstallArgs {
    fn run(&self) -> Result<ExitCode> {
        let root = match &self.dir {
            Some(dir) => explicit_install_root(dir)?,
            None => detect_install_root()?,
        };
        let report = install_bootstrap(&root)?;
        write_stdout(&success_message(&root, &report))?;
        Ok(ExitCode::Success)
    }
}

fn version_relation(reported: Option<u32>, expected: u32) -> VersionRelation {
    match reported {
        None => VersionRelation::Unknown,
        Some(reported) if reported == expected => VersionRelation::Current,
        Some(reported) if reported < expected => VersionRelation::BootstrapOutdated,
        Some(_) => VersionRelation::ToolchainOutdated,
    }
}

fn bootstrap_warning(relation: VersionRelation) -> Option<&'static str> {
    match relation {
        VersionRelation::BootstrapOutdated => Some(
            "> [!WARNING]\n> This project's BAML bootstrap skill is outdated. Ask the user to run `baml agent install`. Continue using the guide below for this session.\n\n",
        ),
        VersionRelation::ToolchainOutdated => Some(
            "> [!NOTE]\n> This project's BAML bootstrap skill is newer than the active BAML toolchain. The guide below still matches this toolchain.\n\n",
        ),
        VersionRelation::Unknown | VersionRelation::Current => None,
    }
}

fn write_stdout(content: &str) -> Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(content.as_bytes())?;
    if !content.ends_with('\n') {
        stdout.write_all(b"\n")?;
    }
    Ok(())
}

fn explicit_install_root(dir: &Path) -> Result<PathBuf> {
    dir.canonicalize()
        .with_context(|| format!("failed to resolve {}", dir.display()))
}

fn detect_install_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("failed to read current directory")?;
    baml_workspace::find_baml_project_root(&cwd).ok_or_else(|| {
        anyhow!(
            "could not find a BAML project from {}; run `baml init` or pass `--project <PATH>`",
            cwd.display()
        )
    })
}

pub(crate) fn warn_if_bootstrap_missing() {
    if !io::stderr().is_terminal() {
        return;
    }
    let Some(root) = std::env::current_dir()
        .ok()
        .and_then(|cwd| baml_workspace::find_baml_project_root(&cwd))
    else {
        return;
    };
    if let Some(warning) = bootstrap_install_warning(&root) {
        crate::reporter::print_warning(format_args!("{warning}"));
    }
}

fn bootstrap_install_warning(root: &Path) -> Option<&'static str> {
    if project_has_active_skill(root, LEGACY_SKILL_NAME) {
        Some("the legacy BAML agent skill is outdated; run `baml agent install`")
    } else if project_has_active_skill(root, SKILL_NAME) {
        None
    } else {
        Some("the BAML agent bootstrap is not installed; run `baml agent install`")
    }
}

fn project_has_active_skill(root: &Path, name: &str) -> bool {
    [".agents/skills", ".claude/skills"]
        .into_iter()
        .map(|skills_dir| root.join(skills_dir).join(name).join("SKILL.md"))
        .any(|path| path.is_file())
}

pub(crate) fn install_bootstrap(root: &Path) -> Result<InstallReport> {
    let mut targets = Vec::with_capacity(2);
    for relative_skills_dir in [
        Path::new(".agents").join("skills"),
        Path::new(".claude").join("skills"),
    ] {
        targets.push(install_bootstrap_to(root, relative_skills_dir, &BOOTSTRAP)?);
    }
    Ok(InstallReport { targets })
}

fn install_bootstrap_to(
    root: &Path,
    relative_skills_dir: PathBuf,
    bootstrap: &BootstrapSpec,
) -> Result<TargetReport> {
    let skills_dir = root.join(relative_skills_dir);
    fs::create_dir_all(&skills_dir)
        .with_context(|| format!("failed to create {}", skills_dir.display()))?;

    let skill_dir = skills_dir.join(SKILL_NAME);
    let skill_path = skill_dir.join("SKILL.md");
    let action = match fs::read(&skill_path) {
        Ok(contents) if contents == bootstrap.contents.as_bytes() => InstallAction::Unchanged,
        Ok(_) => InstallAction::Updated,
        Err(err) if err.kind() == io::ErrorKind::NotFound && skill_dir.exists() => {
            InstallAction::Updated
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => InstallAction::Created,
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read {}", skill_path.display()));
        }
    };

    let tmp_dir = skills_dir.join(format!(".baml-agent-install-{}", std::process::id()));
    if tmp_dir.exists() {
        remove_path(&tmp_dir)
            .with_context(|| format!("failed to remove stale {}", tmp_dir.display()))?;
    }

    let result = (|| -> Result<TargetReport> {
        let mut archived = Vec::new();
        if action != InstallAction::Unchanged {
            let staged_skill_dir = tmp_dir.join(SKILL_NAME);
            fs::create_dir_all(&staged_skill_dir)
                .with_context(|| format!("failed to create {}", staged_skill_dir.display()))?;
            write_atomic(&staged_skill_dir.join("SKILL.md"), bootstrap.contents)?;
            if let Some(path) = replace_skill_dir(&skills_dir, &tmp_dir, SKILL_NAME)? {
                archived.push(path);
            }
        }
        if let Some(path) = archive_existing_skill(&skills_dir, LEGACY_SKILL_NAME)? {
            archived.push(path);
        }
        Ok(TargetReport {
            path: skill_path,
            action,
            archived,
        })
    })();

    let cleanup = remove_path(&tmp_dir);
    match (result, cleanup) {
        (Err(err), _) => Err(err),
        (Ok(_), Err(err)) if err.kind() != std::io::ErrorKind::NotFound => {
            Err(err).context("failed to clean up temporary BAML agent skill directory")
        }
        (Ok(report), _) => Ok(report),
    }
}

fn archive_existing_skill(skills_dir: &Path, name: &str) -> Result<Option<PathBuf>> {
    let active_dir = skills_dir.join(name);
    if !active_dir.exists() {
        return Ok(None);
    }

    let archive_dir = skills_dir.join(OLD_SKILLS_DIR).join(name);
    if archive_dir.exists() {
        remove_path(&archive_dir)
            .with_context(|| format!("failed to clear {}", archive_dir.display()))?;
    }
    let archive_root = archive_dir
        .parent()
        .expect("archived skill path always has a parent");
    fs::create_dir_all(archive_root)
        .with_context(|| format!("failed to create {}", archive_root.display()))?;
    fs::rename(&active_dir, &archive_dir).with_context(|| {
        format!(
            "failed to archive existing {} into {}",
            active_dir.display(),
            archive_dir.display()
        )
    })?;
    Ok(Some(archive_dir))
}

fn replace_skill_dir(skills_dir: &Path, tmp_dir: &Path, name: &str) -> Result<Option<PathBuf>> {
    let final_dir = skills_dir.join(name);
    let next_dir = tmp_dir.join(name);
    let archive = archive_existing_skill(skills_dir, name)?;

    if let Err(err) = fs::rename(&next_dir, &final_dir) {
        let mut error = anyhow!(err).context(format!(
            "failed to install {name} into {}",
            final_dir.display()
        ));
        if let Some(archive_dir) = archive {
            if final_dir.exists() {
                error = error.context(format!(
                    "previous {name} skill remains at {}",
                    archive_dir.display()
                ));
            } else if let Err(restore_err) = fs::rename(&archive_dir, &final_dir) {
                error = error.context(format!(
                    "failed to restore previous {name} skill from {} to {}: {restore_err}",
                    archive_dir.display(),
                    final_dir.display()
                ));
            }
        }
        return Err(error);
    }
    Ok(archive)
}

fn remove_path(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(err) => Err(err),
    }
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

fn success_message(root: &Path, report: &InstallReport) -> String {
    let changed = report
        .targets
        .iter()
        .any(|target| target.action != InstallAction::Unchanged || !target.archived.is_empty());
    let summary = if changed {
        "Installed the BAML agent bootstrap"
    } else {
        "The BAML agent bootstrap is already current"
    };
    let archive_note = if report
        .targets
        .iter()
        .any(|target| !target.archived.is_empty())
    {
        "\n\nReplaced skills are kept in baml-old_skills/ next to the installed skill."
    } else {
        ""
    };
    format!(
        "{summary} in {}\n\nClaude Code:\n  .claude/skills/baml/SKILL.md\n\nCodex / OpenCode:\n  .agents/skills/baml/SKILL.md{archive_note}\n\nRestart any already-running agent session to pick it up.",
        root.display()
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use super::*;

    #[test]
    fn embedded_bootstrap_declares_the_cli_protocol_version() {
        assert!(BOOTSTRAP.contents.contains("\nname: baml\n"));
        assert!(BOOTSTRAP.contents.contains(&format!(
            "baml-bootstrap-version: \"{}\"",
            BOOTSTRAP.version
        )));
        assert!(BOOTSTRAP.contents.contains(&format!(
            "baml agent guide --bootstrap-version {}",
            BOOTSTRAP.version
        )));
    }

    #[test]
    fn bootstrap_version_relation_is_total() {
        assert_eq!(version_relation(None, 2), VersionRelation::Unknown);
        assert_eq!(version_relation(Some(2), 2), VersionRelation::Current);
        assert_eq!(
            version_relation(Some(1), 2),
            VersionRelation::BootstrapOutdated
        );
        assert_eq!(
            version_relation(Some(3), 2),
            VersionRelation::ToolchainOutdated
        );
    }

    #[test]
    fn install_refreshes_baml_and_preserves_unrelated_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let stale = root.join(".agents/skills/baml/SKILL.md");
        fs::create_dir_all(stale.parent().unwrap()).unwrap();
        fs::write(&stale, "stale").unwrap();
        let unrelated = root.join(".agents/skills/other/SKILL.md");
        fs::create_dir_all(unrelated.parent().unwrap()).unwrap();
        fs::write(&unrelated, "keep").unwrap();

        let report = install_bootstrap(root).unwrap();

        assert_eq!(fs::read_to_string(stale).unwrap(), BOOTSTRAP.contents);
        assert_eq!(fs::read_to_string(unrelated).unwrap(), "keep");
        assert_eq!(
            fs::read_to_string(root.join(".agents/skills/baml-old_skills/baml/SKILL.md")).unwrap(),
            "stale"
        );
        assert!(root.join(".claude/skills/baml/SKILL.md").is_file());
        assert_eq!(report.targets[0].action, InstallAction::Updated);
        assert_eq!(report.targets[1].action, InstallAction::Created);
    }

    #[test]
    fn install_archives_the_legacy_baml_core_skill() {
        let tmp = tempfile::tempdir().unwrap();
        for skills_dir in [".agents/skills", ".claude/skills"] {
            let legacy = tmp.path().join(skills_dir).join("baml-core/SKILL.md");
            fs::create_dir_all(legacy.parent().unwrap()).unwrap();
            fs::write(legacy, "legacy").unwrap();
        }

        let first = install_bootstrap(tmp.path()).unwrap();

        for skills_dir in [".agents/skills", ".claude/skills"] {
            let root = tmp.path().join(skills_dir);
            assert!(!root.join("baml-core").exists());
            assert_eq!(
                fs::read_to_string(root.join("baml-old_skills/baml-core/SKILL.md")).unwrap(),
                "legacy"
            );
            assert_eq!(
                fs::read_to_string(root.join("baml/SKILL.md")).unwrap(),
                BOOTSTRAP.contents
            );
        }
        assert!(
            first
                .targets
                .iter()
                .all(|target| target.action == InstallAction::Created)
        );

        let second = install_bootstrap(tmp.path()).unwrap();
        assert!(second.targets.iter().all(|target| {
            target.action == InstallAction::Unchanged && target.archived.is_empty()
        }));
    }

    #[test]
    fn project_skill_detection_ignores_archives() {
        for name in [SKILL_NAME, LEGACY_SKILL_NAME] {
            let tmp = tempfile::tempdir().unwrap();
            let skill = tmp
                .path()
                .join(".agents/skills")
                .join(name)
                .join("SKILL.md");
            fs::create_dir_all(skill.parent().unwrap()).unwrap();
            fs::write(skill, "skill").unwrap();
            assert!(project_has_active_skill(tmp.path(), name));
        }

        let tmp = tempfile::tempdir().unwrap();
        let archived = tmp
            .path()
            .join(".agents/skills/baml-old_skills/baml/SKILL.md");
        fs::create_dir_all(archived.parent().unwrap()).unwrap();
        fs::write(archived, "skill").unwrap();
        assert!(!project_has_active_skill(tmp.path(), SKILL_NAME));
    }

    #[test]
    fn project_warning_distinguishes_missing_current_and_legacy_skills() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            bootstrap_install_warning(tmp.path())
                .unwrap()
                .contains("not installed")
        );

        let current = tmp.path().join(".agents/skills/baml/SKILL.md");
        fs::create_dir_all(current.parent().unwrap()).unwrap();
        fs::write(&current, "bootstrap").unwrap();
        assert_eq!(bootstrap_install_warning(tmp.path()), None);

        let legacy = tmp.path().join(".claude/skills/baml-core/SKILL.md");
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(legacy, "legacy").unwrap();
        assert!(
            bootstrap_install_warning(tmp.path())
                .unwrap()
                .contains("outdated")
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
    fn root_detection_requires_a_baml_project() {
        let _guard = cwd_lock().lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let old = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let error = detect_install_root().unwrap_err();
        std::env::set_current_dir(old).unwrap();

        assert!(format!("{error}").contains("could not find a BAML project"));
    }

    #[test]
    fn explicit_dir_is_used_without_walk_up() {
        let _guard = cwd_lock().lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let explicit = tmp.path().join("custom");
        fs::create_dir(&explicit).unwrap();

        let root = explicit_install_root(&explicit).unwrap();

        assert_eq!(root, explicit.canonicalize().unwrap());
    }

    #[test]
    fn success_message_lists_the_single_installed_skill() {
        let report = InstallReport {
            targets: Vec::new(),
        };
        let message = success_message(Path::new("/tmp/project"), &report);
        assert!(message.contains(".agents/skills/baml/SKILL.md"));
        assert!(message.contains(".claude/skills/baml/SKILL.md"));
        assert!(!message.contains("baml-*"));
    }

    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
}
