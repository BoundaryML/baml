use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use baml_workspace::{find_baml_project_root, resolve_project_search_start};
use clap::Args;

use crate::project_load::{SourceLocation, load_project_from, resolve_source_location};

/// Open the BAML playground in a browser.
///
/// Serves either a discovered BAML project or one standalone source file. By
/// default, the server selects the first available port starting at 4265 and
/// opens a browser unless the session is headless.
#[derive(Args, Clone, Debug)]
#[command(after_long_help = "\
Examples:
  Open the nearest project:
    baml playground

  Serve a specific project without opening a browser:
    baml playground --project ./my-project --no-open

  Serve a standalone file on a fixed port:
    baml playground --file script.baml --port 4265")]
pub struct PlaygroundArgs {
    #[command(flatten)]
    pub compiler: crate::commands::CompilerArgs,

    /// Standalone single-file source. Loads only this file (no project discovery).
    #[arg(long, value_name = "PATH", help_heading = "Project options")]
    pub file: Option<PathBuf>,

    /// Deprecated alias for `--project`.
    #[arg(long, value_name = "PATH", hide = true)]
    pub from: Option<PathBuf>,

    /// Listen on exactly this port (errors if unavailable).
    /// Default: the first free port from 4265.
    #[arg(long, value_name = "PORT", help_heading = "Server options")]
    pub port: Option<u16>,

    /// Do not open a browser. Opening is also skipped automatically in
    /// headless sessions (SSH, or no display on Linux).
    #[arg(long, help_heading = "Server options")]
    pub no_open: bool,
}

impl PlaygroundArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let playground_dir = resolve_playground_assets()?;

        let roots = workspace_roots(self.from.as_deref(), self.file.as_deref())?;
        let options = baml_lsp_server::PlaygroundServerOptions {
            port: self.port,
            open_browser: !self.no_open
                && !is_headless_session(|key| std::env::var_os(key).is_some()),
        };
        baml_lsp_server::run_playground_server(roots, playground_dir, options)?;
        Ok(crate::ExitCode::Success)
    }
}

/// A browser can't usefully open here: an SSH session, or a Linux/BSD session
/// with no display server. `webbrowser` would fall back to text-mode browsers
/// (lynx/w3m) that hijack the terminal. macOS/Windows sessions are never
/// display-less in practice, so only the SSH signal applies there.
fn is_headless_session(has_env: impl Fn(&str) -> bool) -> bool {
    if has_env("SSH_CONNECTION") || has_env("SSH_TTY") {
        return true;
    }
    if cfg!(all(unix, not(target_os = "macos"))) {
        return !has_env("DISPLAY") && !has_env("WAYLAND_DISPLAY");
    }
    false
}

fn workspace_roots(from: Option<&Path>, file: Option<&Path>) -> Result<Vec<PathBuf>> {
    // The playground's LSP currently discovers projects from marker-bearing
    // workspace roots. Unlike the compile/run commands, it cannot yet carry a
    // settings root and a disjoint source root as one project, so retain its
    // marker requirement instead of accepting a root it would silently ignore.
    if file.is_some() {
        return match resolve_source_location(from, file, None)? {
            SourceLocation::StandaloneFile { file, .. } => Ok(vec![file]),
            SourceLocation::Project { .. } => unreachable!("file mode resolved as a project"),
        };
    }

    let search_start = resolve_project_search_start(from)
        .with_context(|| "could not resolve playground project search path")?;
    let Some(marked_root) = find_baml_project_root(&search_start) else {
        anyhow::bail!(
            "`{}` doesn't look like it belongs to a BAML project — no `baml.toml` \
             and no `baml_src/` directory found in it or its ancestors.",
            search_start.display()
        );
    };
    let (_db, root, files) = load_project_from(Some(&marked_root))?;
    if files.is_empty() {
        anyhow::bail!("no `.baml` files found in {}", root.display());
    }
    Ok(vec![root])
}

fn resolve_playground_assets() -> Result<Option<PathBuf>> {
    if std::env::var_os("BAML_PLAYGROUND_DEV_PORT").is_some()
        || std::env::var_os("BAML_PLAYGROUND_DIR").is_some()
    {
        return Ok(None);
    }

    if let Some(dir) = discover_playground_dir()? {
        return Ok(Some(dir));
    }

    anyhow::bail!(
        "could not find packaged playground assets. For local debugging, run \
         `pnpm --filter app-vscode-webview dev -- --host 127.0.0.1 --port 4000` \
         and set `BAML_PLAYGROUND_DEV_PORT=4000`, or set `BAML_PLAYGROUND_DIR` \
         to a built app-vscode-webview/dist directory."
    );
}

fn discover_playground_dir() -> Result<Option<PathBuf>> {
    let exe = std::env::current_exe().context("could not resolve current executable")?;
    let Some(bin_dir) = exe.parent() else {
        return Ok(None);
    };

    Ok(playground_dir_candidates(bin_dir)
        .into_iter()
        .find(|path| is_playground_dir(path)))
}

fn playground_dir_candidates(bin_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    candidates.push(bin_dir.join("../assets/playground"));
    candidates.push(bin_dir.join("../dist/playground"));

    for ancestor in bin_dir.ancestors() {
        candidates.push(ancestor.join("typescript2/app-vscode-webview/dist"));
        candidates.push(ancestor.join("../typescript2/app-vscode-webview/dist"));
        candidates.push(ancestor.join("typescript2/app-vscode-ext/dist/playground"));
        candidates.push(ancestor.join("../typescript2/app-vscode-ext/dist/playground"));
    }

    candidates
}

fn is_playground_dir(path: &Path) -> bool {
    path.join("index.html").is_file()
        && path.join("assets/index.js").is_file()
        && path.join("assets/index.css").is_file()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn valid_manifest() -> &'static str {
        "[package]\nname = \"test-project\"\n"
    }

    fn env_with(present: &'static [&'static str]) -> impl Fn(&str) -> bool {
        move |key| present.contains(&key)
    }

    #[test]
    fn ssh_connection_implies_headless() {
        assert!(is_headless_session(env_with(&[
            "SSH_CONNECTION",
            "DISPLAY"
        ])));
    }

    #[test]
    fn ssh_tty_implies_headless() {
        assert!(is_headless_session(env_with(&["SSH_TTY", "DISPLAY"])));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn display_present_is_not_headless() {
        assert!(!is_headless_session(env_with(&["DISPLAY"])));
        assert!(!is_headless_session(env_with(&["WAYLAND_DISPLAY"])));
        assert!(is_headless_session(env_with(&[])));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn no_signals_is_not_headless_on_macos() {
        assert!(!is_headless_session(env_with(&[])));
    }

    #[test]
    fn project_mode_errors_without_project_marker() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("loose.baml"),
            "function main() -> int { 1 }",
        )
        .unwrap();

        let err = workspace_roots(Some(tmp.path()), None).unwrap_err();
        assert!(format!("{err}").contains("doesn't look like it belongs to a BAML project"));
    }

    #[test]
    fn project_mode_accepts_baml_src_marker() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("baml_src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("main.baml"), "function main() -> int { 1 }").unwrap();

        let roots = workspace_roots(Some(tmp.path()), None).unwrap();
        assert_eq!(roots, vec![std::fs::canonicalize(tmp.path()).unwrap()]);
    }

    #[test]
    fn project_mode_accepts_manifest_marker() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("baml.toml"), valid_manifest()).unwrap();
        fs::write(tmp.path().join("main.baml"), "function main() -> int { 1 }").unwrap();

        let roots = workspace_roots(Some(tmp.path()), None).unwrap();
        assert_eq!(roots, vec![std::fs::canonicalize(tmp.path()).unwrap()]);
    }

    #[test]
    fn project_mode_walks_up_from_baml_src() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("baml.toml"), valid_manifest()).unwrap();
        let src = tmp.path().join("baml_src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("main.baml"), "function main() -> int { 1 }").unwrap();

        let roots = workspace_roots(Some(&src), None).unwrap();
        assert_eq!(roots, vec![std::fs::canonicalize(tmp.path()).unwrap()]);
    }

    #[test]
    fn file_mode_uses_explicit_standalone_file() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("script.baml");
        fs::write(&file, "function main() -> int { 1 }").unwrap();

        let roots = workspace_roots(None, Some(&file)).unwrap();
        assert_eq!(roots, vec![std::fs::canonicalize(file).unwrap()]);
    }

    #[test]
    fn repo_dev_assets_prefer_webview_dist_over_extension_copy() {
        let root = Path::new("/tmp/baml");
        let candidates = playground_dir_candidates(&root.join("baml_language/target/debug"));
        let webview = root.join("typescript2/app-vscode-webview/dist");
        let extension = root.join("typescript2/app-vscode-ext/dist/playground");

        let webview_pos = candidates
            .iter()
            .position(|path| path == &webview)
            .expect("webview dist candidate");
        let extension_pos = candidates
            .iter()
            .position(|path| path == &extension)
            .expect("extension dist candidate");
        assert!(webview_pos < extension_pos);
    }

    #[test]
    fn installed_toolchain_assets_are_first_candidate() {
        let bin_dir = Path::new("/tmp/baml-home/toolchains/1.2.3/bin");
        let candidates = playground_dir_candidates(bin_dir);

        assert_eq!(
            candidates.first().unwrap(),
            &Path::new("/tmp/baml-home/toolchains/1.2.3/bin").join("../assets/playground")
        );
    }

    #[test]
    fn playground_dir_requires_static_bundle() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        fs::create_dir(dir.join("assets")).unwrap();
        fs::write(
            dir.join("index.html"),
            "<script src=\"/assets/index.js\"></script>",
        )
        .unwrap();
        fs::write(dir.join("assets/index.js"), "console.log('ok')").unwrap();

        assert!(!is_playground_dir(dir));

        fs::write(dir.join("assets/index.css"), "body{}").unwrap();
        assert!(is_playground_dir(dir));
    }
}
