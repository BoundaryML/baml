use std::{
    env, fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};

const SKILL_NAME: &str = "baml";
const LEGACY_SKILL_NAME: &str = "baml-core";
const OLD_SKILLS_DIR: &str = "baml-old_skills";
const INSTALL_HELP: &str = r#"Install or refresh the BAML agent bootstrap

Usage:
  baml agent install [--project <PATH>]

The wrapper installs its embedded bootstrap into the detected BAML project.
No network access is required.
"#;
const BOOTSTRAP: BootstrapSpec = BootstrapSpec {
    version: 1,
    contents: include_str!("../../../../skill/bootstrap.md"),
};

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
struct InstallReport {
    targets: Vec<TargetReport>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VersionRelation {
    Unknown,
    Current,
    BootstrapOutdated,
    WrapperOutdated,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum AgentCommand {
    Guide {
        bootstrap_version: Option<u32>,
        forwarded_args: Vec<String>,
    },
    Install {
        project: Option<PathBuf>,
    },
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PassThroughMode {
    Exec,
    InstallBootstrapAfterSuccess { project: PathBuf },
}

pub(crate) fn parse_command(args: &[String]) -> Result<Option<AgentCommand>> {
    let Some((agent_index, "agent")) = top_level_subcommand(args) else {
        return Ok(None);
    };
    let Some((command_index, command)) = next_positional(args, agent_index + 1) else {
        return Ok(None);
    };
    match command {
        "guide" => parse_guide(args, command_index).map(Some),
        "install" if !args[command_index + 1..].iter().any(|arg| is_help_arg(arg)) => {
            parse_install(args, command_index).map(Some)
        }
        _ => Ok(None),
    }
}

pub(crate) fn install_help_requested(args: &[String]) -> bool {
    let Some((agent_index, "agent")) = top_level_subcommand(args) else {
        return false;
    };
    let Some((command_index, "install")) = next_positional(args, agent_index + 1) else {
        return false;
    };
    args[command_index + 1..].iter().any(|arg| is_help_arg(arg))
}

pub(crate) fn print_install_help() {
    print!("{INSTALL_HELP}");
}

fn parse_guide(args: &[String], command_index: usize) -> Result<AgentCommand> {
    let mut forwarded_args = args.to_vec();
    let mut bootstrap_version = None;
    let mut index = command_index + 1;
    while index < forwarded_args.len() {
        if forwarded_args[index] == "--bootstrap-version" {
            let value = forwarded_args
                .get(index + 1)
                .ok_or_else(|| anyhow!("--bootstrap-version requires an unsigned integer value"))?;
            bootstrap_version = Some(parse_bootstrap_version(value)?);
            forwarded_args.drain(index..=index + 1);
        } else if let Some(value) = forwarded_args[index].strip_prefix("--bootstrap-version=") {
            bootstrap_version = Some(parse_bootstrap_version(value)?);
            forwarded_args.remove(index);
        } else {
            index += 1;
        }
    }
    Ok(AgentCommand::Guide {
        bootstrap_version,
        forwarded_args,
    })
}

fn parse_bootstrap_version(value: &str) -> Result<u32> {
    value
        .parse()
        .with_context(|| format!("invalid bootstrap version `{value}`"))
}

fn parse_install(args: &[String], command_index: usize) -> Result<AgentCommand> {
    let mut project = option_path(args, "--project")?;
    let mut index = command_index + 1;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--project" {
            index += 2;
        } else if arg.starts_with("--project=") {
            index += 1;
        } else if arg == "--dir" {
            let value = args
                .get(index + 1)
                .ok_or_else(|| anyhow!("{arg} requires a path"))?;
            if project.replace(PathBuf::from(value)).is_some() {
                anyhow::bail!("project path was provided more than once");
            }
            index += 2;
        } else if let Some(value) = arg.strip_prefix("--dir=") {
            if project.replace(PathBuf::from(value)).is_some() {
                anyhow::bail!("project path was provided more than once");
            }
            index += 1;
        } else if global_flag(arg) {
            index += 1;
        } else if global_value_option(arg).is_some() {
            index += usize::from(!arg.contains('=')) + 1;
        } else {
            anyhow::bail!(
                "usage: baml agent install [--project <PATH>]\nunexpected argument: {arg}"
            );
        }
    }
    let base = invocation_base(args)?;
    project = match project {
        Some(path) if path.is_absolute() => Some(path),
        Some(path) => Some(base.join(path)),
        None if option_path(args, "--directory")?.is_some() => Some(detect_project_root(&base)?),
        None => None,
    };
    Ok(AgentCommand::Install { project })
}

fn top_level_subcommand(args: &[String]) -> Option<(usize, &str)> {
    next_positional(args, 0)
}

fn next_positional(args: &[String], mut index: usize) -> Option<(usize, &str)> {
    while index < args.len() {
        let arg = args[index].as_str();
        if global_flag(arg) {
            index += 1;
        } else if global_value_option(arg).is_some() {
            index += usize::from(!arg.contains('=')) + 1;
        } else if arg == "--" {
            return args.get(index + 1).map(|value| (index + 1, value.as_str()));
        } else if arg.starts_with('-') {
            return None;
        } else {
            return Some((index, arg));
        }
    }
    None
}

fn global_flag(arg: &str) -> bool {
    matches!(arg, "-q" | "--quiet" | "-v" | "--verbose" | "--no-progress")
        || (arg.starts_with('-')
            && !arg.starts_with("--")
            && arg[1..].chars().all(|ch| matches!(ch, 'q' | 'v')))
}

fn global_value_option(arg: &str) -> Option<&'static str> {
    [
        "--directory",
        "--project",
        "--output-preset",
        "--color",
        "--hyperlinks",
        "--diagnostic-format",
    ]
    .into_iter()
    .find(|name| arg == *name || arg.starts_with(&format!("{name}=")))
}

fn option_path(args: &[String], name: &str) -> Result<Option<PathBuf>> {
    let mut value = None;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        let found = if arg == name {
            index += 1;
            Some(
                args.get(index)
                    .ok_or_else(|| anyhow!("{name} requires a path"))?
                    .as_str(),
            )
        } else {
            arg.strip_prefix(&format!("{name}="))
        };
        if let Some(found) = found {
            if value.replace(PathBuf::from(found)).is_some() {
                anyhow::bail!("{name} was provided more than once");
            }
        }
        index += 1;
    }
    Ok(value)
}

fn invocation_base(args: &[String]) -> Result<PathBuf> {
    let cwd = env::current_dir().context("failed to read current directory")?;
    Ok(match option_path(args, "--directory")? {
        Some(path) if path.is_absolute() => path,
        Some(path) => cwd.join(path),
        None => cwd,
    })
}

fn is_help_arg(arg: &str) -> bool {
    matches!(arg, "--help" | "-h" | "help")
}

pub(crate) fn write_version_warning(bootstrap_version: Option<u32>) -> Result<()> {
    let Some(warning) = bootstrap_warning(version_relation(bootstrap_version, BOOTSTRAP.version))
    else {
        return Ok(());
    };
    let mut stdout = io::stdout().lock();
    stdout.write_all(warning.as_bytes())?;
    stdout.flush()?;
    Ok(())
}

fn version_relation(reported: Option<u32>, expected: u32) -> VersionRelation {
    match reported {
        None => VersionRelation::Unknown,
        Some(reported) if reported == expected => VersionRelation::Current,
        Some(reported) if reported < expected => VersionRelation::BootstrapOutdated,
        Some(_) => VersionRelation::WrapperOutdated,
    }
}

fn bootstrap_warning(relation: VersionRelation) -> Option<&'static str> {
    match relation {
        VersionRelation::BootstrapOutdated => Some(
            "WARNING: This BAML bootstrap skill is outdated. It is highly recommended that you run `baml agent install` from the project root before continuing. Continuing with outdated BAML instructions is not recommended.\n\n",
        ),
        VersionRelation::WrapperOutdated => Some(
            "NOTE: This project's BAML bootstrap skill is newer than the active BAML wrapper. The guide below still matches the selected BAML toolchain.\n\n",
        ),
        VersionRelation::Unknown | VersionRelation::Current => None,
    }
}

pub(crate) fn run_install(project: Option<&Path>) -> Result<()> {
    let root = match project {
        Some(path) => path
            .canonicalize()
            .with_context(|| format!("failed to resolve {}", path.display()))?,
        None => {
            detect_project_root(&env::current_dir().context("failed to read current directory")?)?
        }
    };
    let report = install_bootstrap(&root)?;
    println!("{}", success_message(&root, &report));
    Ok(())
}

pub(crate) fn pass_through_mode(args: &[String]) -> PassThroughMode {
    let project = match top_level_subcommand(args) {
        Some((command_index, "init")) => scaffold_project(args, command_index, Path::new(".")),
        Some((command_index, "new")) => scaffold_project(args, command_index, Path::new("")),
        _ => None,
    };
    match project {
        Some(project) => PassThroughMode::InstallBootstrapAfterSuccess { project },
        None => PassThroughMode::Exec,
    }
}

fn scaffold_project(args: &[String], command_index: usize, default: &Path) -> Option<PathBuf> {
    let mut project = (!default.as_os_str().is_empty()).then(|| default.to_path_buf());
    let mut index = command_index + 1;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--name" {
            index += 2;
        } else if arg.starts_with("--name=") || global_flag(arg) {
            index += 1;
        } else if global_value_option(arg).is_some() {
            index += usize::from(!arg.contains('=')) + 1;
        } else if arg == "--" {
            project = args.get(index + 1).map(PathBuf::from);
            break;
        } else if arg.starts_with('-') {
            return None;
        } else if project.is_none() || default == Path::new(".") {
            project = Some(PathBuf::from(arg));
            break;
        } else {
            return None;
        }
    }
    let base = invocation_base(args).ok()?;
    project.map(|path| {
        if path.is_absolute() {
            path
        } else {
            base.join(path)
        }
    })
}

impl PassThroughMode {
    pub(crate) fn after_success(self) -> Result<()> {
        match self {
            Self::Exec => Ok(()),
            Self::InstallBootstrapAfterSuccess { project } => {
                let root = project
                    .canonicalize()
                    .with_context(|| format!("failed to resolve {}", project.display()))?;
                install_bootstrap(&root).map(|_| ())
            }
        }
    }
}

pub(crate) fn warn_if_bootstrap_missing(args: &[String]) {
    let command = top_level_subcommand(args).map(|(_, command)| command);
    if !io::stderr().is_terminal()
        || !matches!(
            command,
            Some("check" | "run" | "generate" | "test" | "pack" | "fmt" | "format" | "playground")
        )
    {
        return;
    }
    let Ok(cwd) = invocation_base(args) else {
        return;
    };
    let Ok(root) = detect_project_root(&cwd) else {
        return;
    };
    if let Some(warning) = bootstrap_install_warning(&root) {
        eprintln!("{}: {warning}", super::warning_prefix());
    }
}

fn detect_project_root(start: &Path) -> Result<PathBuf> {
    let manifest = super::find_project_manifest(start).ok_or_else(|| {
        anyhow!(
            "could not find a BAML project from {}; run `baml init` or pass `--project <PATH>`",
            start.display()
        )
    })?;
    manifest
        .parent()
        .expect("baml.toml always has a parent")
        .canonicalize()
        .with_context(|| {
            format!(
                "failed to resolve project containing {}",
                manifest.display()
            )
        })
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

fn install_bootstrap(root: &Path) -> Result<InstallReport> {
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
        (Ok(_), Err(err)) if err.kind() != io::ErrorKind::NotFound => {
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
    use super::*;

    #[test]
    fn embedded_bootstrap_declares_the_wrapper_protocol_version() {
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
    fn guide_parsing_strips_the_wrapper_protocol_argument() {
        let args = vec![
            "agent".to_string(),
            "guide".to_string(),
            "main".to_string(),
            "--bootstrap-version".to_string(),
            "3".to_string(),
        ];
        assert_eq!(
            parse_command(&args).unwrap(),
            Some(AgentCommand::Guide {
                bootstrap_version: Some(3),
                forwarded_args: vec!["agent".to_string(), "guide".to_string(), "main".to_string(),],
            })
        );
    }

    #[test]
    fn install_parsing_accepts_global_options_before_and_after_agent() {
        let cwd = env::current_dir().unwrap();
        let before = vec![
            "--project".to_string(),
            "before".to_string(),
            "agent".to_string(),
            "install".to_string(),
        ];
        assert_eq!(
            parse_command(&before).unwrap(),
            Some(AgentCommand::Install {
                project: Some(cwd.join("before")),
            })
        );

        let after = vec![
            "-qv".to_string(),
            "agent".to_string(),
            "--quiet".to_string(),
            "install".to_string(),
            "--project=after".to_string(),
        ];
        assert_eq!(
            parse_command(&after).unwrap(),
            Some(AgentCommand::Install {
                project: Some(cwd.join("after")),
            })
        );
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
            VersionRelation::WrapperOutdated
        );
    }

    #[test]
    fn scaffold_commands_wait_for_bootstrap_installation() {
        let cwd = env::current_dir().unwrap();
        assert_eq!(
            pass_through_mode(&["init".to_string()]),
            PassThroughMode::InstallBootstrapAfterSuccess {
                project: cwd.clone(),
            }
        );
        assert_eq!(
            pass_through_mode(&["new".to_string(), "demo".to_string()]),
            PassThroughMode::InstallBootstrapAfterSuccess {
                project: cwd.join("demo"),
            }
        );
        assert_eq!(
            pass_through_mode(&["check".to_string()]),
            PassThroughMode::Exec
        );

        let nested = vec![
            "--directory".to_string(),
            "workspace".to_string(),
            "new".to_string(),
            "--name".to_string(),
            "demo".to_string(),
            "project".to_string(),
        ];
        assert_eq!(
            pass_through_mode(&nested),
            PassThroughMode::InstallBootstrapAfterSuccess {
                project: cwd.join("workspace/project"),
            }
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
    fn install_archives_legacy_baml_core_and_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        for skills_dir in [".agents/skills", ".claude/skills"] {
            let legacy = tmp.path().join(skills_dir).join("baml-core/SKILL.md");
            fs::create_dir_all(legacy.parent().unwrap()).unwrap();
            fs::write(legacy, "legacy").unwrap();
        }

        let first = install_bootstrap(tmp.path()).unwrap();
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
}
