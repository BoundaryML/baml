use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Args;

/// Open the trace-first BAML Studio.
#[derive(Args, Clone, Debug)]
#[command(after_long_help = "\
Examples:
  Open Studio for the nearest project or trace directory:
    baml studio

  Open a retained trace project without compiling sources:
    baml studio ./incident-export

  Serve Studio without opening a browser:
    baml studio --no-open --port 4265")]
pub struct StudioArgs {
    /// Project or ancestor containing `.baml/`.
    #[arg(value_name = "PATH", help_heading = "Project options")]
    pub path: Option<PathBuf>,

    /// Deprecated project spelling accepted for global `--project` plumbing.
    #[arg(long, value_name = "PATH", hide = true)]
    pub from: Option<PathBuf>,

    /// Listen on exactly this port (errors if unavailable).
    #[arg(long, value_name = "PORT", help_heading = "Server options")]
    pub port: Option<u16>,

    /// Do not open a browser.
    #[arg(long, help_heading = "Server options")]
    pub no_open: bool,
}

impl StudioArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let assets = super::playground_command::resolve_playground_assets()?;
        let selected = self.path.as_deref().or(self.from.as_deref());
        let root = resolve_studio_root(selected)?;
        baml_lsp_server::run_studio_server(
            vec![root],
            assets,
            baml_lsp_server::PlaygroundServerOptions {
                port: self.port,
                open_browser: !self.no_open
                    && !super::playground_command::is_headless_session(|key| {
                        std::env::var_os(key).is_some()
                    }),
                landing_path: "/studio".to_owned(),
            },
        )?;
        Ok(crate::ExitCode::Success)
    }
}

fn resolve_studio_root(path: Option<&Path>) -> Result<PathBuf> {
    let selected = path.map_or_else(
        || std::env::current_dir().context("could not resolve current directory"),
        |path| {
            std::fs::canonicalize(path)
                .with_context(|| format!("could not resolve Studio path {}", path.display()))
        },
    )?;
    if !selected.is_dir() {
        anyhow::bail!("Studio path `{}` is not a directory", selected.display());
    }
    for ancestor in selected.ancestors() {
        if ancestor
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == ".baml")
        {
            return Ok(ancestor.parent().unwrap_or(ancestor).to_path_buf());
        }
        if ancestor.join(".baml").is_dir()
            || ancestor.join("baml.toml").is_file()
            || ancestor.join("baml_src").is_dir()
        {
            return Ok(ancestor.to_path_buf());
        }
    }
    anyhow::bail!(
        "`{}` is not inside a BAML project or trace directory; expected `.baml/`, \
         `baml.toml`, or `baml_src/`",
        selected.display()
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn trace_only_project_is_valid_without_sources() {
        let temp = TempDir::new().unwrap();
        let nested = temp.path().join("nested");
        fs::create_dir_all(temp.path().join(".baml/history")).unwrap();
        fs::create_dir_all(&nested).unwrap();
        assert_eq!(
            resolve_studio_root(Some(&nested)).unwrap(),
            std::fs::canonicalize(temp.path()).unwrap()
        );
    }

    #[test]
    fn dot_baml_path_resolves_to_its_owner() {
        let temp = TempDir::new().unwrap();
        let state = temp.path().join(".baml");
        fs::create_dir_all(&state).unwrap();
        assert_eq!(
            resolve_studio_root(Some(&state)).unwrap(),
            std::fs::canonicalize(temp.path()).unwrap()
        );
    }
}
