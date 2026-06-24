use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Args;

use crate::project_load::{SourceLocation, resolve_source_location};

#[derive(Args, Clone, Debug)]
pub struct PlaygroundArgs {
    /// Standalone single-file source. Loads only this file (no project discovery).
    #[arg(long, value_name = "PATH")]
    pub file: Option<PathBuf>,

    /// Project search starting point. Ignored when `--file` is set.
    #[arg(long, value_name = "PATH")]
    pub from: Option<PathBuf>,
}

impl PlaygroundArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let playground_dir = resolve_playground_assets()?;

        let roots = workspace_roots(self.from.as_deref(), self.file.as_deref())?;
        baml_lsp_server::run_playground_server(roots, playground_dir)?;
        Ok(crate::ExitCode::Success)
    }
}

fn workspace_roots(from: Option<&Path>, file: Option<&Path>) -> Result<Vec<PathBuf>> {
    let location = resolve_source_location(from, file, None)?;
    match location {
        SourceLocation::Project { root, files } => {
            if files.is_empty() {
                anyhow::bail!("No .baml files found in {}", root.display());
            }
            Ok(vec![root])
        }
        SourceLocation::StandaloneFile { file, .. } => Ok(vec![file]),
    }
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
        "Could not find packaged playground assets. For local debugging, run \
         `pnpm --filter app-vscode-webview dev -- --host 127.0.0.1 --port 4000` \
         and set `BAML_PLAYGROUND_DEV_PORT=4000`, or set `BAML_PLAYGROUND_DIR` \
         to a built app-vscode-webview/dist directory."
    );
}

fn discover_playground_dir() -> Result<Option<PathBuf>> {
    let exe = std::env::current_exe().context("Could not resolve current executable")?;
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
