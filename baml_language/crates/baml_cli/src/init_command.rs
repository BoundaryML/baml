// `baml init` — scaffold a new BAML project.
//
// Mirrors `cargo init`'s shape: produces a `baml.toml` with a
// `[package]` table at the target directory, plus an empty `baml_src/`
// so the project marker layout (`baml.toml` required + optional
// `baml_src/` for sources) is in place from the start. A starter
// `baml_src/main.baml` gives the user something runnable immediately.
//
// Refuses to overwrite existing `baml.toml`. Directory creation is
// idempotent — running `init` inside an empty existing folder is fine.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Args;

use crate::reporter::Reporter;

/// `baml init` — scaffold a new project under the given directory
/// (default `.`). Refuses to clobber an existing `baml.toml`.
#[derive(Args, Clone, Debug)]
pub struct InitArgs {
    /// Directory to initialize. Defaults to the current directory.
    #[arg(value_name = "PATH", default_value = ".")]
    pub path: PathBuf,

    /// Project name written to `baml.toml`'s `[package].name`. Defaults
    /// to the basename of `<PATH>` (or `baml-project` for `.`).
    #[arg(long)]
    pub name: Option<String>,
}

impl InitArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let reporter = Reporter::new();
        self.run_with_reporter(&reporter)
    }

    fn run_with_reporter(&self, reporter: &Reporter) -> Result<crate::ExitCode> {
        std::fs::create_dir_all(&self.path)
            .with_context(|| format!("Failed to create directory {}", self.path.display()))?;
        let canonical = std::fs::canonicalize(&self.path)
            .with_context(|| format!("Failed to canonicalize path {}", self.path.display()))?;

        let toml_path = canonical.join("baml.toml");
        if toml_path.exists() {
            anyhow::bail!(
                "`{}` already exists. Refusing to overwrite an existing project.",
                toml_path.display()
            );
        }

        let name = self
            .name
            .clone()
            .or_else(|| default_project_name(&canonical))
            .unwrap_or_else(|| "baml-project".to_string());
        validate_project_name(&name)?;

        reporter.spin("Creating", format!("baml.toml ({name})"));
        std::fs::write(&toml_path, render_baml_toml(&name))
            .with_context(|| format!("Failed to write {}", toml_path.display()))?;

        let src_dir = canonical.join("baml_src");
        std::fs::create_dir_all(&src_dir)
            .with_context(|| format!("Failed to create {}", src_dir.display()))?;

        let main_path = src_dir.join("main.baml");
        if !main_path.exists() {
            reporter.spin("Creating", "baml_src/main.baml");
            std::fs::write(&main_path, STARTER_MAIN_BAML)
                .with_context(|| format!("Failed to write {}", main_path.display()))?;
        }

        reporter.finish("Initialized", format!("{} ({name})", canonical.display()));
        Ok(crate::ExitCode::Success)
    }
}

/// Derive a project name from the canonical path's basename. Returns
/// `None` if the path has no usable basename (e.g. `/`), letting the
/// caller fall back to a hard-coded default.
fn default_project_name(canonical: &Path) -> Option<String> {
    canonical
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Cargo-style project-name rules: non-empty, ASCII alphanumeric plus
/// `-`, `_`, `.`. Whitelist beats blacklist here — `render_baml_toml`
/// drops the name straight into a `"..."` TOML string, so a stray `"`
/// (or any control char) in the name produces an unparseable manifest.
fn validate_project_name(name: &str) -> Result<()> {
    if name.is_empty() {
        anyhow::bail!("Project name cannot be empty.");
    }
    let bad: Vec<char> = name
        .chars()
        .filter(|c| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
        .collect();
    if !bad.is_empty() {
        anyhow::bail!(
            "Project name `{name}` contains invalid character(s) {bad:?}. \
             Use ASCII letters, digits, `-`, `_`, or `.`."
        );
    }
    Ok(())
}

fn render_baml_toml(name: &str) -> String {
    format!(
        "[package]\n\
         name = \"{name}\"\n\
         \n\
         # [scripts]\n\
         # dev = \"-f main\"\n",
    )
}

const STARTER_MAIN_BAML: &str = r#"function main() -> string {
  "hello from baml"
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn init_args(path: PathBuf) -> InitArgs {
        InitArgs { path, name: None }
    }

    /// Happy path: empty dir → `baml.toml`, `baml_src/main.baml` written.
    #[test]
    fn init_creates_baml_toml_and_starter_file() {
        let tmp = tempfile::tempdir().unwrap();
        init_args(tmp.path().to_path_buf()).run().unwrap();
        let toml = std::fs::read_to_string(tmp.path().join("baml.toml")).unwrap();
        assert!(toml.contains("[package]"));
        assert!(toml.contains("name = "));
        assert!(tmp.path().join("baml_src/main.baml").exists());
    }

    /// Default name comes from the directory's basename.
    #[test]
    fn init_default_name_matches_directory_basename() {
        let tmp = tempfile::tempdir().unwrap();
        let sub = tmp.path().join("my-cool-app");
        std::fs::create_dir(&sub).unwrap();
        init_args(sub.clone()).run().unwrap();
        let toml = std::fs::read_to_string(sub.join("baml.toml")).unwrap();
        assert!(
            toml.contains("name = \"my-cool-app\""),
            "expected basename-derived name; got:\n{toml}"
        );
    }

    /// Explicit `--name` overrides the directory basename.
    #[test]
    fn init_explicit_name_overrides_default() {
        let tmp = tempfile::tempdir().unwrap();
        let mut args = init_args(tmp.path().to_path_buf());
        args.name = Some("explicit".to_string());
        args.run().unwrap();
        let toml = std::fs::read_to_string(tmp.path().join("baml.toml")).unwrap();
        assert!(toml.contains("name = \"explicit\""));
    }

    /// Refuses to overwrite existing `baml.toml`. No partial writes.
    #[test]
    fn init_refuses_to_clobber_existing_baml_toml() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("baml.toml"), "[package]\nname=\"prior\"\n").unwrap();
        let err = init_args(tmp.path().to_path_buf()).run().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("already exists"), "got: {msg}");

        // The pre-existing file must be untouched.
        let preserved = std::fs::read_to_string(tmp.path().join("baml.toml")).unwrap();
        assert!(preserved.contains("prior"));
    }

    /// Idempotent on `baml_src/` — running init when `baml_src/` already
    /// exists is fine as long as `baml.toml` doesn't (and an existing
    /// `main.baml` isn't overwritten).
    #[test]
    fn init_preserves_existing_main_baml() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("baml_src")).unwrap();
        std::fs::write(
            tmp.path().join("baml_src/main.baml"),
            "// pre-existing content",
        )
        .unwrap();

        init_args(tmp.path().to_path_buf()).run().unwrap();

        let content = std::fs::read_to_string(tmp.path().join("baml_src/main.baml")).unwrap();
        assert_eq!(content, "// pre-existing content");
    }

    /// Whitespace / slashes in `--name` are rejected.
    #[test]
    fn init_rejects_invalid_project_name() {
        let tmp = tempfile::tempdir().unwrap();
        let mut args = init_args(tmp.path().to_path_buf());
        args.name = Some("bad name".to_string());
        let err = args.run().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("invalid character"), "got: {msg}");
    }

    #[test]
    fn validate_project_name_accepts_common_shapes() {
        for ok in &["app", "my-app", "my_app", "my.app", "App1", "a"] {
            validate_project_name(ok).expect(ok);
        }
    }

    #[test]
    fn validate_project_name_rejects_empty_and_path_separators() {
        assert!(validate_project_name("").is_err());
        assert!(validate_project_name("a/b").is_err());
        assert!(validate_project_name("a\\b").is_err());
        assert!(validate_project_name("a b").is_err());
    }

    /// `render_baml_toml` drops the name straight into a `"..."` literal,
    /// so a name containing `"` would produce unparseable TOML. The
    /// validator's whitelist catches this before we ever try to write.
    #[test]
    fn validate_project_name_rejects_toml_breakers() {
        assert!(
            validate_project_name(r#"my"app"#).is_err(),
            "double quote must be rejected (would break TOML literal)",
        );
        assert!(
            validate_project_name("my'app").is_err(),
            "single quote must be rejected",
        );
        assert!(
            validate_project_name("my\nname").is_err(),
            "newline must be rejected",
        );
        assert!(
            validate_project_name("my\tname").is_err(),
            "tab must be rejected",
        );
    }

    /// Non-ASCII names are rejected by the whitelist — keep package
    /// names ASCII so they map cleanly onto filesystem-name conventions
    /// the binary outputs use.
    #[test]
    fn validate_project_name_rejects_non_ascii() {
        assert!(validate_project_name("café").is_err());
        assert!(validate_project_name("プロジェクト").is_err());
    }

    /// Whatever the validator rejects, the `baml init` happy path must
    /// also reject. Defensive against the validator and the init flow
    /// drifting apart.
    #[test]
    fn init_rejects_quote_in_name() {
        let tmp = tempfile::tempdir().unwrap();
        let mut args = init_args(tmp.path().to_path_buf());
        args.name = Some(r#"my"app"#.into());
        let err = args.run().unwrap_err();
        assert!(format!("{err}").contains("invalid character"));
    }
}
