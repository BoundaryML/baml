use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, anyhow};
use clap::Args;

use crate::ExitCode;

pub(crate) const SKILL_NAME: &str = "baml-core";
pub(crate) const SKILL_CONTENT: &str = include_str!("../../../../skills/baml-core/SKILL.md");
const EMBEDDED_SKILLS: &[Skill<'static>] = &[Skill {
    name: SKILL_NAME,
    content: SKILL_CONTENT,
}];

#[derive(Args, Clone, Debug)]
pub(crate) struct AgentArgs {
    #[command(subcommand)]
    pub command: AgentCommand,
}

#[derive(clap::Subcommand, Clone, Debug)]
pub(crate) enum AgentCommand {
    #[command(about = "Install the BAML agent skill bundled with this toolchain")]
    Install(AgentInstallArgs),
}

/// Install or refresh the BAML agent skill bundled with this toolchain.
#[derive(Args, Clone, Debug)]
#[command(after_long_help = "\
Examples:
  Install the bundled skill:
    baml agent install

  Install the bundled skill in a specific project:
    baml agent install --project ./my-project")]
pub(crate) struct AgentInstallArgs {
    /// Deprecated alias for `--project`.
    #[arg(long, value_name = "PATH", hide = true)]
    pub dir: Option<PathBuf>,
}

#[derive(Debug)]
struct Skill<'a> {
    name: &'a str,
    content: &'a str,
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
        install_skills(&root, EMBEDDED_SKILLS)?;
        print_success(&root)?;
        Ok(ExitCode::Success)
    }
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
/// Inside a git repo, the nearest baml.toml (else baml_src) owner wins, but
/// the walk never leaves the repo; with no project marker the repo root is
/// used, since that is where agent harnesses discover `.claude/skills/`.
/// Outside a git repo the walk stops before the user's home directory —
/// a stray `~/baml_src` must not pull installs into `$HOME` — or, when no
/// home directory is known, doesn't leave the current directory at all; the
/// fallback is always the current directory. All inputs must be
/// pre-canonicalized.
fn detect_install_root_in(cwd: &Path, git_toplevel: Option<&Path>, home: Option<&Path>) -> PathBuf {
    // A toplevel that doesn't contain cwd (an exported GIT_DIR/GIT_WORK_TREE
    // pointing at a dotfiles worktree in $HOME) is not the project being
    // worked in; neither is one rooted at — or containing — the home
    // directory itself, which would reopen the stray-~/baml_src hole this
    // function exists to close. Ignore both rather than install there.
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
        // No git scope and no known home boundary: a walk could climb
        // arbitrarily far and adopt a stray marker directory, so don't walk
        // at all. Installing at cwd is always safe.
        (None, None) => vec![cwd.to_path_buf()],
    };
    baml_db::find_baml_project_root_from_ancestors(
        ancestors,
        |dir| dir.join(baml_db::BAML_TOML).is_file(),
        |dir| dir.join(baml_db::BAML_SRC_DIR).is_dir(),
    )
    .unwrap_or_else(|| match git_toplevel {
        Some(toplevel) => toplevel.to_path_buf(),
        None => cwd.to_path_buf(),
    })
}

/// Canonicalized toplevel of the git repo containing `dir`, if any.
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

/// The user's canonicalized home directory (`HOME`, or `USERPROFILE` on
/// Windows). Not [`baml_release::baml_home`], which is `~/.baml` state
/// storage and overridable via `BAML_HOME`.
fn user_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .and_then(|home| home.canonicalize().ok())
}

fn install_skills(root: &Path, skills: &[Skill<'_>]) -> Result<()> {
    install_skills_to(root, Path::new(".agents").join("skills"), skills)?;
    install_skills_to(root, Path::new(".claude").join("skills"), skills)?;
    Ok(())
}

fn install_skills_to(
    root: &Path,
    relative_skills_dir: PathBuf,
    skills: &[Skill<'_>],
) -> Result<()> {
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
            write_atomic(&skill_dir.join("SKILL.md"), skill.content)?;
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

/// Directory (inside each skills dir) where the previous version of a skill
/// is kept when an install replaces it. One slot per skill: each install
/// overwrites the slot with the version it just replaced. The name doesn't
/// clash with real skills because archived copies sit one level deeper than
/// the `<skills>/<name>/SKILL.md` layout agent harnesses discover.
const OLD_SKILLS_DIR: &str = "baml-old_skills";

fn replace_skill_dir(skills_dir: &Path, tmp_dir: &Path, skill: &Skill<'_>) -> Result<()> {
    let final_dir = skills_dir.join(skill.name);
    let next_dir = tmp_dir.join(skill.name);
    let mut archive = None;

    if final_dir.exists() {
        let archive_dir = skills_dir.join(OLD_SKILLS_DIR).join(skill.name);
        if archive_dir.exists() {
            fs::remove_dir_all(&archive_dir).with_context(|| {
                format!("failed to clear old-skill slot {}", archive_dir.display())
            })?;
        }
        if let Some(parent) = archive_dir.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::rename(&final_dir, &archive_dir).with_context(|| {
            format!(
                "failed to archive existing {} into {}",
                final_dir.display(),
                archive_dir.display()
            )
        })?;
        archive = Some(archive_dir);
    }

    if let Err(err) = fs::rename(&next_dir, &final_dir) {
        let mut error = anyhow!(err).context(format!(
            "failed to install {} into {}",
            skill.name,
            final_dir.display()
        ));
        if let Some(archive_dir) = archive {
            if final_dir.exists() {
                error = error.context(format!(
                    "previous {} skill remains at {}",
                    skill.name,
                    archive_dir.display()
                ));
            } else if let Err(restore_err) = fs::rename(&archive_dir, &final_dir) {
                error = error.context(format!(
                    "failed to restore previous {} skill from {} to {}: {restore_err}",
                    skill.name,
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
        "Installed the BAML agent skill bundled with this toolchain in {}\n\nClaude Code:\n  .claude/skills/baml-core/SKILL.md\n\nCodex / OpenCode:\n  .agents/skills/baml-core/SKILL.md\n\nReplaced skills are kept in baml-old_skills/ next to the new one.\n\nRestart any already-running agent session to pick it up.",
        root.display()
    )
}

fn print_success(root: &Path) -> Result<()> {
    writeln!(std::io::stdout(), "{}", success_message(root))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use super::*;

    #[test]
    fn embedded_skill_name_matches_install_directory() {
        assert!(SKILL_CONTENT.starts_with("---\nname: baml-core\n"));
    }

    #[test]
    fn install_writes_embedded_skill_and_preserves_unrelated_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let unrelated = tmp.path().join(".agents/skills/other/SKILL.md");
        fs::create_dir_all(unrelated.parent().unwrap()).unwrap();
        fs::write(&unrelated, "other").unwrap();

        install_skills(tmp.path(), EMBEDDED_SKILLS).unwrap();

        for skills_dir in [".agents/skills", ".claude/skills"] {
            assert_eq!(
                fs::read_to_string(
                    tmp.path()
                        .join(skills_dir)
                        .join(SKILL_NAME)
                        .join("SKILL.md")
                )
                .unwrap(),
                SKILL_CONTENT
            );
        }
        assert_eq!(fs::read_to_string(unrelated).unwrap(), "other");
    }

    #[test]
    fn old_skill_archive_keeps_only_the_previous_version() {
        let tmp = tempfile::tempdir().unwrap();
        let install = |content| {
            install_skills(
                tmp.path(),
                &[Skill {
                    name: SKILL_NAME,
                    content,
                }],
            )
            .unwrap();
        };

        install("v1");
        install("v2");
        install("v3");

        for skills_dir in [".agents/skills", ".claude/skills"] {
            let dir = tmp.path().join(skills_dir);
            assert_eq!(
                fs::read_to_string(dir.join(SKILL_NAME).join("SKILL.md")).unwrap(),
                "v3"
            );
            assert_eq!(
                fs::read_to_string(dir.join(OLD_SKILLS_DIR).join(SKILL_NAME).join("SKILL.md"))
                    .unwrap(),
                "v2"
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
        // A baml_src above the repo must not pull the install outside it.
        fs::create_dir_all(outer.join("baml_src")).unwrap();
        let toplevel = outer.join("repo");
        let cwd = toplevel.join("sub");
        fs::create_dir_all(&cwd).unwrap();

        let root = detect_install_root_in(&cwd, Some(&toplevel), None);

        assert_eq!(root, toplevel);
    }

    #[test]
    fn install_root_ignores_git_toplevel_that_is_not_an_ancestor() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        // Dotfiles bare-repo pattern: exported GIT_DIR/GIT_WORK_TREE make git
        // report a worktree (often $HOME) unrelated to the directory the user
        // is actually in. The project markers next to cwd must still win.
        let foreign_worktree = base.join("home");
        let project = base.join("opt/project");
        let cwd = project.join("sub");
        fs::create_dir_all(&cwd).unwrap();
        fs::create_dir_all(&foreign_worktree).unwrap();
        fs::write(project.join("baml.toml"), "[package]\nname = \"x\"\n").unwrap();

        let root = detect_install_root_in(&cwd, Some(&foreign_worktree), Some(&foreign_worktree));

        assert_eq!(root, project);
    }

    #[test]
    fn install_root_ignores_git_toplevel_at_or_above_home() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().canonicalize().unwrap();
        // A repo rooted at $HOME (dotfiles-as-git-repo) must not turn the
        // stray-marker hole back on: the home boundary still applies.
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
        // No git scope and no home boundary: never adopt a marker above cwd.
        fs::create_dir_all(base.join("baml_src")).unwrap();
        let cwd = base.join("a/b");
        fs::create_dir_all(&cwd).unwrap();

        let root = detect_install_root_in(&cwd, None, None);

        assert_eq!(root, cwd);
    }

    #[test]
    fn install_root_ignores_stray_baml_src_at_home() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().canonicalize().unwrap();
        // Regression: a stray ~/baml_src used to make installs from ~/some/dir
        // land the skills in $HOME.
        fs::create_dir_all(home.join("baml_src")).unwrap();
        let cwd = home.join("nomad");
        fs::create_dir_all(&cwd).unwrap();

        let root = detect_install_root_in(&cwd, None, Some(&home));

        assert_eq!(root, cwd);
    }

    #[test]
    fn install_root_accepts_baml_src_owner_below_home() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().canonicalize().unwrap();
        let project = home.join("work");
        let cwd = project.join("sub");
        fs::create_dir_all(&cwd).unwrap();
        fs::create_dir_all(project.join("baml_src")).unwrap();

        let root = detect_install_root_in(&cwd, None, Some(&home));

        assert_eq!(root, project);
    }

    #[test]
    fn install_root_in_home_itself_is_home() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().canonicalize().unwrap();
        fs::create_dir_all(home.join("baml_src")).unwrap();

        let root = detect_install_root_in(&home, None, Some(&home));

        assert_eq!(root, home);
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
    fn success_message_names_embedded_skill_paths() {
        let message = success_message(Path::new("/tmp/project"));
        assert!(message.contains("bundled with this toolchain"), "{message}");
        assert!(
            message.contains(".agents/skills/baml-core/SKILL.md"),
            "{message}"
        );
        assert!(
            message.contains(".claude/skills/baml-core/SKILL.md"),
            "{message}"
        );
    }

    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
}
