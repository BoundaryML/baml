use std::{
    env,
    ffi::OsString,
    fs,
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

/// Install the active toolchain's BAML editor extension.
///
/// Select Cursor or VS Code explicitly, or use `--output-dir` to copy the VSIX for a
/// manual installation. With no option, BAML selects an available supported
/// editor.
#[derive(Args, Clone, Debug)]
#[command(after_long_help = "\
Examples:
  Install into the detected editor:
    baml ide install

  Install into Cursor:
    baml ide install --cursor

  Copy the extension for manual installation:
    baml ide install --output-dir ./extensions")]
pub(crate) struct IdeInstallArgs {
    /// Install the active toolchain's BAML VSIX into Cursor.
    #[arg(long, help_heading = "Editor options")]
    pub cursor: bool,
    /// Install the active toolchain's BAML VSIX into VS Code.
    #[arg(long, help_heading = "Editor options")]
    pub code: bool,
    /// Copy the active toolchain's BAML VSIX into a directory for manual
    /// install.
    #[arg(
        long = "output-dir",
        alias = "dir",
        value_name = "PATH",
        help_heading = "Editor options"
    )]
    pub dir: Option<PathBuf>,
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
        if let Some(dir) = &self.dir {
            fs::create_dir_all(dir)
                .with_context(|| format!("failed to create {}", dir.display()))?;
            let dest = dir.join("baml-vscode.vsix");
            fs::copy(&vsix, &dest).with_context(|| {
                format!("failed to copy {} to {}", vsix.display(), dest.display())
            })?;
            #[allow(clippy::print_stdout)]
            {
                println!("extracted BAML IDE extension to {}", dest.display());
            }
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

        #[allow(clippy::print_stdout)]
        {
            println!("installed BAML IDE extension from {}", vsix.display());
        }
        Ok(ExitCode::Success)
    }

    fn resolve_editor(&self) -> Result<OsString> {
        if self.cursor {
            return command_on_path("cursor")
                .ok_or_else(|| missing_ide_cli_error("Cursor", "cursor", HostOs::CURRENT));
        }
        if self.code {
            return command_on_path("code")
                .ok_or_else(|| missing_ide_cli_error("VS Code", "code", HostOs::CURRENT));
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
                r"failed to install the BAML extension automatically: no supported IDE CLI (`code` or `cursor`) was found on PATH.

{help}",
                help = manual_install_help("VS Code or Cursor", HostOs::CURRENT)
            )),
        }
    }
}

/// The IDE is often installed without its CLI shim (on macOS the `code`
/// and `cursor` commands must be added to PATH by hand from inside the
/// IDE), so a bare not-found error dead-ends users who do have the IDE.
/// Explain what we could not do and walk through the manual install
/// instead.
fn missing_ide_cli_error(ide: &str, cli: &str, os: HostOs) -> anyhow::Error {
    anyhow!(
        r"failed to install the BAML extension into {ide} automatically: the `{cli}` CLI was not found on PATH.

{help}",
        help = manual_install_help(ide, os)
    )
}

/// OS-specific wording for the manual-install guidance: the example
/// downloads directory and the Command Palette chord. A parameter of
/// [`manual_install_help`] so tests can cover every variant.
#[derive(Clone, Copy, Debug)]
enum HostOs {
    MacOs,
    Windows,
    Linux,
}

impl HostOs {
    /// The OS this binary was built for. Toolchains are per-platform, so
    /// compile-time selection matches the machine the user is running on.
    const CURRENT: Self = if cfg!(target_os = "macos") {
        Self::MacOs
    } else if cfg!(windows) {
        Self::Windows
    } else {
        Self::Linux
    };

    /// A directory that exists on (or is at least meaningful to) every
    /// desktop install of this OS.
    fn example_dir(self) -> &'static str {
        match self {
            Self::Windows => r"%USERPROFILE%\Downloads",
            Self::MacOs | Self::Linux => "~/Downloads",
        }
    }

    fn palette_chord(self) -> &'static str {
        match self {
            Self::MacOs => "Cmd+Shift+P",
            Self::Windows | Self::Linux => "Ctrl+Shift+P",
        }
    }
}

fn manual_install_help(ide: &str, os: HostOs) -> String {
    format!(
        r#"Install it manually instead:

  1. Save baml-vscode.vsix somewhere easy to find:

         baml ide install --output-dir {dir}

  2. In {ide}, press {chord} and run "Extensions: Install from VSIX...".

  3. Select the saved baml-vscode.vsix."#,
        dir = os.example_dir(),
        chord = os.palette_chord(),
    )
}

fn active_toolchain_vsix() -> Result<PathBuf> {
    let exe = env::current_exe().context("failed to locate baml-cli executable")?;
    let toolchain_root = exe
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| anyhow!("failed to determine active BAML toolchain root"))?;
    let vsix = toolchain_root.join("assets").join("baml-vscode.vsix");
    if !vsix.exists() {
        // A local build has no assets/ next to it, so say that plainly rather
        // than reporting a missing file the developer never expected to exist.
        if let Some(local) = env::var_os("BAML_WRAPPER_LOCAL_TOOLCHAIN") {
            anyhow::bail!(
                "baml ide install needs a managed BAML toolchain, but the active one is a local binary at {}.\nThe VS Code extension ships with released toolchains only.\nRun: baml toolchain use canary",
                Path::new(&local).display()
            );
        }
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

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[derive(Parser, Debug)]
    struct Wrapper {
        #[command(flatten)]
        args: IdeInstallArgs,
    }

    #[test]
    fn install_dir_flag_binds() {
        let parsed = Wrapper::try_parse_from(["install", "--output-dir", "out"]).unwrap();
        assert_eq!(parsed.args.dir, Some(PathBuf::from("out")));
    }

    /// Pin the missing-CLI error verbatim so any wording change is a
    /// deliberate one. Windows exercises the OS-specific pieces (example
    /// directory and Command Palette chord) that differ from the Unix
    /// defaults.
    #[test]
    fn missing_ide_cli_error_suggests_manual_install() {
        let msg = missing_ide_cli_error("VS Code", "code", HostOs::Windows).to_string();
        assert_eq!(
            msg,
            r#"failed to install the BAML extension into VS Code automatically: the `code` CLI was not found on PATH.

Install it manually instead:

  1. Save baml-vscode.vsix somewhere easy to find:

         baml ide install --output-dir %USERPROFILE%\Downloads

  2. In VS Code, press Ctrl+Shift+P and run "Extensions: Install from VSIX...".

  3. Select the saved baml-vscode.vsix."#
        );
    }
}
