//! `baml studio [PATH]` — the §9.1 observability-first entry (P8).
//!
//! Starts the same server `baml playground` starts and lands on the runs
//! list (`/studio`). Unlike the playground, studio opens any directory
//! containing `.baml/` even when it has no compilable sources — it is a
//! trace viewer first, an authoring surface second.

use std::{
    net::{Ipv4Addr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use clap::Args;

/// Open BAML Studio, the runs/trace viewer, in a browser.
///
/// Serves the same server as `baml playground` with the runs list as the
/// landing page. Accepts any directory containing `.baml/` observability
/// data, even one with no compilable BAML sources.
#[derive(Args, Clone, Debug)]
#[command(after_long_help = "\
Examples:
  Open studio for the nearest project:
    baml studio

  Open a directory that only has `.baml/` trace data:
    baml studio ./captured-run

  Serve without opening a browser, on a fixed port:
    baml studio --no-open --port 4265")]
pub struct StudioArgs {
    /// Project directory, or any directory containing `.baml/` trace data.
    /// Default: discover the project from the current directory upward.
    #[arg(value_name = "PATH")]
    pub path: Option<PathBuf>,

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

impl StudioArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let start = self.start_path()?;
        let (root, trace_viewer_only) = match resolve_studio_root(start)? {
            StudioRoot::Project(root) => (root, false),
            StudioRoot::TraceViewer(root) => (root, true),
        };

        let playground_dir = crate::playground_command::resolve_playground_assets()?;

        let port = match self.port {
            Some(port) => port,
            None => pick_free_port(4265, 100)?,
        };
        let url = format!("http://localhost:{port}/studio");

        if trace_viewer_only {
            crate::reporter::print_warning(format_args!(
                "no BAML project markers found; serving `{}` as a trace viewer \
                 (recorded runs only, nothing to compile or run)",
                root.display()
            ));
        }
        #[allow(clippy::print_stdout)] // user-facing banner, like the playground's
        {
            println!("\n  Studio:      {url}");
        }

        if !self.no_open
            && !crate::playground_command::is_headless_session(|key| {
                std::env::var_os(key).is_some()
            })
        {
            spawn_browser_opener(port, url);
        }

        // The same server `baml playground` starts. `open_browser: false`
        // because the server's own opener lands on `/` (the playground);
        // studio opens `/studio` itself (above) once the port accepts
        // connections.
        baml_lsp_server::run_playground_server(
            vec![root],
            playground_dir,
            baml_lsp_server::PlaygroundServerOptions {
                port: Some(port),
                open_browser: false,
            },
        )?;
        Ok(crate::ExitCode::Success)
    }

    /// The positional PATH and `--project` name the same thing; accept one.
    fn start_path(&self) -> Result<Option<&Path>> {
        match (self.path.as_deref(), self.from.as_deref()) {
            (Some(_), Some(_)) => anyhow::bail!(
                "PATH and `--project` both name the directory to open; pass only one."
            ),
            (Some(path), None) => Ok(Some(path)),
            (None, from) => Ok(from),
        }
    }
}

/// How studio interprets the resolved directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StudioRoot {
    /// A real BAML project root (`baml.toml` / `baml_src/`), found with the
    /// same upward walk the playground's project resolution uses.
    Project(PathBuf),
    /// No project markers anywhere up the tree, but a directory owns
    /// `.baml/` observability data: serve it as a pure trace viewer.
    TraceViewer(PathBuf),
}

/// Resolve what `baml studio` should serve, starting from `from` (or the
/// current directory when omitted).
///
/// 1. Project resolution, exactly like the playground: walk up for
///    `baml.toml` / `baml_src/`. Unlike the playground, an empty project
///    (no `.baml` sources) is *not* rejected — recorded traces are enough.
/// 2. Trace-viewer mode (§9.1): otherwise accept a directory whose only
///    credential is containing a `.baml/` directory. Deliberately checked on
///    the start directory only, with **no** upward walk: `~/.baml` is the
///    CLI's home config dir (toolchains, credentials), so an ancestor scan
///    would misread the user's home as a trace store.
pub(crate) fn resolve_studio_root(from: Option<&Path>) -> Result<StudioRoot> {
    if let Some(root) = crate::project_load::find_project_root_from(from)? {
        return Ok(StudioRoot::Project(root));
    }

    let canonical =
        baml_workspace::resolve_project_search_start(from).with_context(|| match from {
            Some(from) => format!("could not resolve path: {}", from.display()),
            None => "could not resolve current directory".to_string(),
        })?;
    let start_dir = baml_workspace::project_search_dir(&canonical);
    if start_dir.join(".baml").is_dir() {
        return Ok(StudioRoot::TraceViewer(start_dir));
    }
    anyhow::bail!(
        "`{}` is not a BAML project (no `baml.toml` or `baml_src/` here or in \
         its ancestors) and contains no `.baml/` observability data.\n\
         run `baml init` to create a project, or point `baml studio <PATH>` at a \
         directory containing `.baml/`. To edit a single file, use \
         `baml playground --file <PATH>`.",
        start_dir.display()
    );
}

/// Find a free loopback port starting at `base_port` — the same scan
/// `run_playground_server` performs when no port is given. Studio picks the
/// port up front so it can print and open the `/studio` URL; the server then
/// binds exactly that port.
fn pick_free_port(base_port: u16, max_attempts: u16) -> Result<u16> {
    for offset in 0..max_attempts {
        let port = base_port + offset;
        if TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok() {
            return Ok(port);
        }
    }
    anyhow::bail!(
        "could not find an available port in range {base_port}..{}; pass --port",
        base_port + max_attempts
    )
}

/// Open `url` once the server accepts connections on `port`. Runs on its own
/// thread: `run_playground_server` blocks the main thread, and
/// `webbrowser::open` can itself block until a text-mode browser exits.
fn spawn_browser_opener(port: u16, url: String) {
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_ok() {
                if let Err(error) = webbrowser::open(&url) {
                    crate::reporter::print_warning(format_args!(
                        "could not open a browser at {url}: {error}"
                    ));
                }
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn valid_manifest() -> &'static str {
        "[package]\nname = \"test-project\"\n"
    }

    fn canonical(path: &Path) -> PathBuf {
        fs::canonicalize(path).unwrap()
    }

    #[test]
    fn resolves_manifest_project_root() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("baml.toml"), valid_manifest()).unwrap();

        let root = resolve_studio_root(Some(tmp.path())).unwrap();
        assert_eq!(root, StudioRoot::Project(canonical(tmp.path())));
    }

    #[test]
    fn resolves_baml_src_project_root_from_nested_dir() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("baml_src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("main.baml"), "function main() -> int { 1 }").unwrap();

        let root = resolve_studio_root(Some(&src)).unwrap();
        assert_eq!(root, StudioRoot::Project(canonical(tmp.path())));
    }

    /// §9.1: a directory whose only credential is containing `.baml/` is a
    /// valid studio target (trace-viewer mode) even with zero sources.
    #[test]
    fn baml_dir_alone_enters_trace_viewer_mode() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join(".baml")).unwrap();

        let root = resolve_studio_root(Some(tmp.path())).unwrap();
        assert_eq!(root, StudioRoot::TraceViewer(canonical(tmp.path())));
    }

    /// Trace-viewer detection is deliberately non-recursive: an upward walk
    /// would misread `~/.baml` (the CLI's home config dir — toolchains,
    /// credentials) as a trace store. A nested start dir must therefore be
    /// rejected, not resolved to some `.baml`-owning ancestor.
    #[test]
    fn trace_viewer_mode_does_not_walk_up() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join(".baml")).unwrap();
        let nested = tmp.path().join("sub").join("deeper");
        fs::create_dir_all(&nested).unwrap();

        let err = resolve_studio_root(Some(&nested)).unwrap_err();
        assert!(format!("{err}").contains(".baml"), "got: {err}");
    }

    /// Real project markers take precedence over `.baml/`: studio then also
    /// offers the full playground surface, not just traces.
    #[test]
    fn project_markers_win_over_trace_viewer_mode() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("baml.toml"), valid_manifest()).unwrap();
        fs::create_dir(tmp.path().join(".baml")).unwrap();

        let root = resolve_studio_root(Some(tmp.path())).unwrap();
        assert_eq!(root, StudioRoot::Project(canonical(tmp.path())));
    }

    /// Unlike `baml playground`, a project with markers but no `.baml`
    /// source files is accepted — studio is a trace viewer first.
    #[test]
    fn sourceless_project_is_accepted() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("baml.toml"), valid_manifest()).unwrap();

        assert!(matches!(
            resolve_studio_root(Some(tmp.path())).unwrap(),
            StudioRoot::Project(_)
        ));
    }

    #[test]
    fn bare_dir_is_rejected_with_remedies() {
        let tmp = TempDir::new().unwrap();

        let err = resolve_studio_root(Some(tmp.path())).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains(".baml"), "got: {msg}");
        assert!(msg.contains("baml init"), "got: {msg}");
        assert!(msg.contains("baml playground"), "got: {msg}");
    }

    #[test]
    fn missing_path_is_a_clear_error() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("nope");
        let err = resolve_studio_root(Some(&missing)).unwrap_err();
        assert!(format!("{err:#}").contains("could not resolve path"));
    }

    fn args_with(path: Option<PathBuf>, from: Option<PathBuf>) -> StudioArgs {
        StudioArgs {
            path,
            from,
            port: None,
            no_open: true,
        }
    }

    #[test]
    fn positional_path_and_project_flag_conflict() {
        let args = args_with(Some(PathBuf::from("a")), Some(PathBuf::from("b")));
        let err = args.start_path().unwrap_err();
        assert!(format!("{err}").contains("pass only one"));
    }

    #[test]
    fn positional_path_is_preferred_start() {
        let args = args_with(Some(PathBuf::from("a")), None);
        assert_eq!(args.start_path().unwrap(), Some(Path::new("a")));
    }

    #[test]
    fn project_flag_is_the_fallback_start() {
        let args = args_with(None, Some(PathBuf::from("b")));
        assert_eq!(args.start_path().unwrap(), Some(Path::new("b")));

        let args = args_with(None, None);
        assert_eq!(args.start_path().unwrap(), None);
    }
}
