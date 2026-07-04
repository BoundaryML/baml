use std::{
    env,
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, anyhow};
use clap::Args;

use crate::ExitCode;

#[derive(Args, Clone, Debug)]
pub(crate) struct IdeArgs {
    #[command(subcommand)]
    pub command: IdeCommand,
}

#[derive(clap::Subcommand, Clone, Debug)]
pub(crate) enum IdeCommand {
    #[command(about = "Install the active toolchain's BAML IDE extension")]
    Install(IdeInstallArgs),
}

#[derive(Args, Clone, Debug)]
pub(crate) struct IdeInstallArgs {
    /// Install the active toolchain's BAML VSIX into Cursor.
    #[arg(long)]
    pub cursor: bool,
    /// Install the active toolchain's BAML VSIX into VS Code.
    #[arg(long, conflicts_with = "cursor")]
    pub code: bool,
    /// Copy the active toolchain's BAML VSIX into a directory for manual install.
    #[arg(long, value_name = "DIR", conflicts_with_all = ["cursor", "code"])]
    pub extract: Option<PathBuf>,
}

impl IdeArgs {
    pub fn run(&self) -> Result<ExitCode> {
        match &self.command {
            IdeCommand::Install(args) => args.run(),
        }
    }
}

impl IdeInstallArgs {
    pub fn run(&self) -> Result<ExitCode> {
        let vsix = active_toolchain_vsix()?;
        if let Some(dir) = &self.extract {
            fs::create_dir_all(dir)
                .with_context(|| format!("failed to create {}", dir.display()))?;
            let dest = dir.join("baml-vscode.vsix");
            fs::copy(&vsix, &dest).with_context(|| {
                format!("failed to copy {} to {}", vsix.display(), dest.display())
            })?;
            writeln!(
                std::io::stdout(),
                "extracted BAML IDE extension to {}",
                dest.display()
            )?;
            return Ok(ExitCode::Success);
        }

        let editor = self.resolve_editor()?;
        let status = Command::new(&editor)
            .arg("--install-extension")
            .arg(&vsix)
            .status()
            .with_context(|| format!("failed to run {}", editor.to_string_lossy()))?;

        if !status.success() {
            anyhow::bail!(
                "{} --install-extension {} exited with {status}",
                editor.to_string_lossy(),
                vsix.display()
            );
        }

        writeln!(
            std::io::stdout(),
            "installed BAML IDE extension from {}",
            vsix.display()
        )?;
        Ok(ExitCode::Success)
    }

    fn resolve_editor(&self) -> Result<OsString> {
        if self.cursor {
            return Ok(OsString::from("cursor"));
        }
        if self.code {
            return command_on_path("code")
                .ok_or_else(|| anyhow!("VS Code CLI `code` was not found on PATH"));
        }

        let cursor = command_on_path("cursor");
        let code = command_on_path("code");
        match (cursor, code) {
            (Some(cursor), None) => Ok(cursor),
            (None, Some(code)) => Ok(code),
            (Some(_), Some(_)) => Err(anyhow!(
                "both Cursor and VS Code CLIs were found; rerun with --cursor or --code"
            )),
            (None, None) => Err(anyhow!(
                "no supported editor CLI was found.\nManual install commands:\n  cursor --install-extension {}\n  code --install-extension {}",
                active_toolchain_vsix()?.display(),
                active_toolchain_vsix()?.display()
            )),
        }
    }
}

fn active_toolchain_vsix() -> Result<PathBuf> {
    let exe = env::current_exe().context("failed to locate baml-cli executable")?;
    let toolchain_root = exe
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| anyhow!("failed to determine active BAML toolchain root"))?;
    let vsix = toolchain_root.join("assets").join("baml-vscode.vsix");
    if !vsix.exists() {
        anyhow::bail!(
            "active BAML toolchain does not include assets/baml-vscode.vsix at {}",
            vsix.display()
        );
    }
    Ok(vsix)
}

fn command_on_path(command: &str) -> Option<OsString> {
    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        #[cfg(windows)]
        {
            let candidate = dir.join(format!("{command}.cmd"));
            if candidate.exists() {
                return Some(OsString::from(format!("{command}.cmd")));
            }
        }
        let candidate = dir.join(command);
        if candidate.exists() {
            return Some(OsString::from(command));
        }
    }
    None
}
