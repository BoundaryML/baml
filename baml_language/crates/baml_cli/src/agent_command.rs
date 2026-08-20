use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, anyhow};
use clap::Args;

use crate::ExitCode;

const SKILL_NAME: &str = "baml";
const LEGACY_SKILL_NAME: &str = "baml-core";
const OLD_SKILLS_DIR: &str = "baml-old_skills";
const SKILL_STUB: &str = include_str!("../../../../skill/stub.md");
const MAIN_GUIDE: &str = include_str!("../../../../skill/guides/main.md");

#[derive(Args, Clone, Debug)]
pub(crate) struct AgentArgs {
    #[command(subcommand)]
    pub command: AgentCommand,
}

#[derive(clap::Subcommand, Clone, Debug)]
pub(crate) enum AgentCommand {
    #[command(about = "Print the agent guide bundled with this BAML toolchain")]
    Guide(AgentGuideArgs),

    #[command(about = "Install or refresh the BAML agent skill in this project")]
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
}

#[derive(Args, Clone, Debug)]
#[command(after_long_help = "\
Examples:
  Install the BAML skill:
    baml agent install

  Install the skill in a specific project:
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
        install_skill(&root)?;
        write_stdout(&success_message(&root))?;
        Ok(ExitCode::Success)
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
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    dir.canonicalize()
        .with_context(|| format!("failed to resolve {}", dir.display()))
}

fn detect_install_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("failed to read current directory")?;
    let canonical = cwd
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", cwd.display()))?;
    Ok(detect_install_root_in(
        &canonical,
        git_toplevel(&canonical).as_deref(),
        user_home_dir().as_deref(),
    ))
}

/// Pick the install root for project-local agent skills.
///
/// Inside a git repo, the nearest `baml.toml` owner, then the nearest
/// `baml_src` owner, wins. The walk never leaves the repo. Outside a git repo,
/// the walk stops before the user's home directory. All inputs must be
/// canonicalized.
fn detect_install_root_in(cwd: &Path, git_toplevel: Option<&Path>, home: Option<&Path>) -> PathBuf {
    let git_toplevel = git_toplevel.filter(|toplevel| {
        cwd.starts_with(toplevel) && !home.is_some_and(|home| home.starts_with(toplevel))
    });
    let ancestors: Vec<PathBuf> = match (git_toplevel, home) {
        (Some(toplevel), _) => cwd
            .ancestors()
            .take_while(|dir| dir.starts_with(toplevel))
            .map(Path::to_path_buf)
            .collect(),
        (None, Some(home)) => cwd
            .ancestors()
            .take_while(|dir| *dir != home)
            .map(Path::to_path_buf)
            .collect(),
        (None, None) => vec![cwd.to_path_buf()],
    };
    baml_workspace::find_baml_project_root_from_ancestors(
        ancestors,
        |dir| dir.join(baml_workspace::BAML_TOML).is_file(),
        |dir| dir.join(baml_workspace::BAML_SRC_DIR).is_dir(),
    )
    .unwrap_or_else(|| match git_toplevel {
        Some(toplevel) => toplevel.to_path_buf(),
        None => cwd.to_path_buf(),
    })
}

fn git_toplevel(dir: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let root = stdout.trim();
    if root.is_empty() {
        return None;
    }
    PathBuf::from(root).canonicalize().ok()
}

fn user_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .and_then(|home| home.canonicalize().ok())
}

fn install_skill(root: &Path) -> Result<()> {
    install_skill_to(root, Path::new(".agents").join("skills"))?;
    install_skill_to(root, Path::new(".claude").join("skills"))?;
    Ok(())
}

fn install_skill_to(root: &Path, relative_skills_dir: PathBuf) -> Result<()> {
    let skills_dir = root.join(relative_skills_dir);
    fs::create_dir_all(&skills_dir)
        .with_context(|| format!("failed to create {}", skills_dir.display()))?;

    let tmp_dir = skills_dir.join(format!(".baml-agent-install-{}", std::process::id()));
    if tmp_dir.exists() {
        fs::remove_dir_all(&tmp_dir)
            .with_context(|| format!("failed to remove stale {}", tmp_dir.display()))?;
    }
    let staged_skill_dir = tmp_dir.join(SKILL_NAME);
    fs::create_dir_all(&staged_skill_dir)
        .with_context(|| format!("failed to create {}", staged_skill_dir.display()))?;
    write_atomic(&staged_skill_dir.join("SKILL.md"), SKILL_STUB)?;

    let result = (|| -> Result<()> {
        replace_skill_dir(&skills_dir, &tmp_dir, SKILL_NAME)?;
        archive_existing_skill(&skills_dir, LEGACY_SKILL_NAME)?;
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

fn archive_existing_skill(skills_dir: &Path, name: &str) -> Result<Option<PathBuf>> {
    let active_dir = skills_dir.join(name);
    if !active_dir.exists() {
        return Ok(None);
    }

    let archive_dir = skills_dir.join(OLD_SKILLS_DIR).join(name);
    if archive_dir.exists() {
        fs::remove_dir_all(&archive_dir)
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

fn replace_skill_dir(skills_dir: &Path, tmp_dir: &Path, name: &str) -> Result<()> {
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
    Ok(())
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

fn success_message(root: &Path) -> String {
    format!(
        "Installed the BAML agent skill in {}\n\nClaude Code:\n  .claude/skills/baml/SKILL.md\n\nCodex / OpenCode:\n  .agents/skills/baml/SKILL.md\n\nReplaced skills are kept in baml-old_skills/ next to the installed skill.\n\nRestart any already-running agent session to pick it up.",
        root.display()
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use super::*;

    #[test]
    fn embedded_stub_uses_the_baml_name_and_dynamic_guide() {
        assert!(SKILL_STUB.contains("\nname: baml\n"));
        assert!(SKILL_STUB.contains("baml agent guide"));
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

        install_skill(root).unwrap();

        assert_eq!(fs::read_to_string(stale).unwrap(), SKILL_STUB);
        assert_eq!(fs::read_to_string(unrelated).unwrap(), "keep");
        assert_eq!(
            fs::read_to_string(root.join(".agents/skills/baml-old_skills/baml/SKILL.md")).unwrap(),
            "stale"
        );
        assert!(root.join(".claude/skills/baml/SKILL.md").is_file());
    }

    #[test]
    fn install_archives_the_legacy_baml_core_skill() {
        let tmp = tempfile::tempdir().unwrap();
        for skills_dir in [".agents/skills", ".claude/skills"] {
            let legacy = tmp.path().join(skills_dir).join("baml-core/SKILL.md");
            fs::create_dir_all(legacy.parent().unwrap()).unwrap();
            fs::write(legacy, "legacy").unwrap();
        }

        install_skill(tmp.path()).unwrap();

        for skills_dir in [".agents/skills", ".claude/skills"] {
            let root = tmp.path().join(skills_dir);
            assert!(!root.join("baml-core").exists());
            assert_eq!(
                fs::read_to_string(root.join("baml-old_skills/baml-core/SKILL.md")).unwrap(),
                "legacy"
            );
            assert_eq!(
                fs::read_to_string(root.join("baml/SKILL.md")).unwrap(),
                SKILL_STUB
            );
        }
    }

    #[test]
    fn install_root_prefers_baml_toml_within_git_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let toplevel = tmp.path().canonicalize().unwrap();
        let project = toplevel.join("services/x");
        let cwd = project.join("deep");
        fs::create_dir_all(&cwd).unwrap();
        fs::write(project.join("baml.toml"), "[package]\nname = \"x\"\n").unwrap();

        let root = detect_install_root_in(&cwd, Some(&toplevel), None);

        assert_eq!(root, project);
    }

    #[test]
    fn install_root_never_escapes_git_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let outer = tmp.path().canonicalize().unwrap();
        fs::create_dir_all(outer.join("baml_src")).unwrap();
        let toplevel = outer.join("repo");
        let cwd = toplevel.join("sub");
        fs::create_dir_all(&cwd).unwrap();

        let root = detect_install_root_in(&cwd, Some(&toplevel), None);

        assert_eq!(root, toplevel);
    }

    #[test]
    fn install_root_ignores_git_toplevel_at_or_above_home() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().canonicalize().unwrap();
        fs::create_dir_all(home.join("baml_src")).unwrap();
        let cwd = home.join("nomad");
        fs::create_dir_all(&cwd).unwrap();

        let root = detect_install_root_in(&cwd, Some(&home), Some(&home));

        assert_eq!(root, cwd);
    }

    #[test]
    fn install_root_without_known_home_stays_at_cwd() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        fs::create_dir_all(base.join("baml_src")).unwrap();
        let cwd = base.join("a/b");
        fs::create_dir_all(&cwd).unwrap();

        let root = detect_install_root_in(&cwd, None, None);

        assert_eq!(root, cwd);
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
    fn explicit_dir_is_used_without_walk_up() {
        let _guard = cwd_lock().lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let explicit = tmp.path().join("custom");

        let root = explicit_install_root(&explicit).unwrap();

        assert_eq!(root, explicit.canonicalize().unwrap());
    }

    #[test]
    fn success_message_lists_the_single_installed_skill() {
        let message = success_message(Path::new("/tmp/project"));
        assert!(message.contains(".agents/skills/baml/SKILL.md"));
        assert!(message.contains(".claude/skills/baml/SKILL.md"));
        assert!(!message.contains("baml-*"));
    }

    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
}
