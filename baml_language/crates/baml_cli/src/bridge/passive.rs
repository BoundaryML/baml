//! Passive bridge-freshness warning, printed on the ordinary authoring
//! commands.
//!
//! Editing `baml_src/` silently invalidates the generated bridge, and today
//! the mismatch only surfaces at runtime. Rather than requiring anyone to
//! remember a verification step, any command surfaces it — so an agent never
//! has to reason about whether to check.
//!
//! Structurally this is [`crate::skill_check::SkillCheck`]: an RAII guard with
//! `start()` / `skipped()`, doing its work on a background thread and printing
//! in `Drop` (after the command's own output) within a bounded budget.
//!
//! Deliberately absent: an environment-variable opt-out. If one is wanted it
//! belongs in `baml.toml`, next to the rest of the bridge policy.

use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, channel},
    time::{Duration, Instant},
};

use super::status;

/// How long [`BridgeCheck::drop`] waits for the background scan once the
/// command has finished. The scan reads every `.baml` file, which is
/// milliseconds on real projects; the bound exists so a pathological tree can
/// never hold up a command's exit.
const BUDGET: Duration = Duration::from_millis(750);

pub(crate) struct BridgeCheck {
    scan: Option<(Receiver<Vec<String>>, Instant)>,
}

impl BridgeCheck {
    /// Start the freshness scan for the project rooted at `from`.
    pub(crate) fn start(from: Option<PathBuf>) -> Self {
        let deadline = Instant::now() + BUDGET;
        let (sender, receiver) = channel();
        std::thread::spawn(move || {
            let _ = sender.send(stale_warnings(from));
        });
        Self {
            scan: Some((receiver, deadline)),
        }
    }

    /// An inert guard, for machine-facing commands, account commands, the
    /// long-running editor surfaces (`playground`, `lsp`) where a startup
    /// line is noise, and `bridge` itself — which either fixes the staleness
    /// or reports it precisely.
    pub(crate) fn skipped() -> Self {
        Self { scan: None }
    }
}

/// One warning per out-of-date bridge. Empty whenever there is nothing
/// actionable to say.
fn stale_warnings(from: Option<PathBuf>) -> Vec<String> {
    // Every failure here is silent: this is an unsolicited nudge on somebody
    // else's command, so a project it cannot resolve is simply not its
    // business.
    let Ok(Some(layout)) = crate::project_load::resolve_project_layout(from.as_deref()) else {
        return Vec::new();
    };
    if !layout.root.join("baml.toml").is_file() {
        return Vec::new();
    }
    let (generators, _) = crate::generate::discover_generators(&layout.root);
    if generators.is_empty() {
        return Vec::new();
    }
    let Ok(fingerprint) = super::fingerprint::compute(&layout.root, &layout.source_root) else {
        return Vec::new();
    };

    generators
        .iter()
        .filter_map(|generator| {
            // Manifest depth only: one small file read per bridge, so this
            // stays off the `baml check` latency budget. `--check` is the
            // one that re-hashes every generated file.
            let status = status::evaluate(
                &generator.output_dir,
                &fingerprint,
                baml_version::CANONICAL_VERSION,
                status::Depth::Manifest,
            )
            .ok()?;
            match status {
                status::Status::Fresh => None,
                // A configured generator with no bridge on disk is the fresh
                // clone case (the default `vcs = "ignore"` keeps the tree out
                // of the repo), which is exactly when the nudge is useful. A
                // newly initialized project is not nagged, because `baml
                // init` writes its generator lines commented out and the
                // no-generators check above already returned.
                status::Status::NeverGenerated => Some(format!(
                    "generated bridge `{}` has not been generated yet; run `baml bridge generate`",
                    generator.name
                )),
                status::Status::Stale(reasons) => Some(status::warning(&generator.name, &reasons)),
            }
        })
        .collect()
}

impl Drop for BridgeCheck {
    fn drop(&mut self) {
        let Some((receiver, deadline)) = self.scan.take() else {
            return;
        };
        let remaining = deadline.saturating_duration_since(Instant::now());
        let Ok(warnings) = receiver.recv_timeout(remaining) else {
            return;
        };
        for warning in warnings {
            crate::reporter::print_warning(format_args!("{warning}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use baml_codegen_types::{
        GeneratedOutputFile, OutputOptions, OutputProvenance, VcsPolicy, write_generated_output,
    };

    use super::*;

    fn project(generator: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("baml.toml"),
            format!("[package]\nname = \"test\"\n{generator}"),
        )
        .unwrap();
        let source_root = dir.path().join("baml_src");
        fs::create_dir_all(&source_root).unwrap();
        fs::write(source_root.join("main.baml"), "class A {}").unwrap();
        dir
    }

    const PYTHON_GENERATOR: &str = "\n[generator.client1]\noutput_type = \"python/pydantic\"\n\
         output_dir = \".\"\nnaming_convention = \"preserve-case\"\n";

    fn generate(dir: &std::path::Path, fingerprint: &str) {
        let (generators, _) = crate::generate::discover_generators(dir);
        write_generated_output(
            &generators[0].output_dir,
            vec![GeneratedOutputFile::new("value.py", "generated")],
            &OutputOptions {
                provenance: OutputProvenance {
                    input_fingerprint: fingerprint.to_string(),
                    toolchain_version: baml_version::CANONICAL_VERSION.to_string(),
                    generator_name: generators[0].name.clone(),
                },
                vcs: VcsPolicy::Ignore,
            },
        )
        .unwrap();
    }

    fn current_fingerprint(dir: &std::path::Path) -> String {
        super::super::fingerprint::compute(dir, &dir.join("baml_src")).unwrap()
    }

    #[test]
    fn a_project_without_a_manifest_is_silent() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("baml_src")).unwrap();
        fs::write(dir.path().join("baml_src/main.baml"), "class A {}").unwrap();

        assert!(stale_warnings(Some(dir.path().to_path_buf())).is_empty());
    }

    #[test]
    fn a_project_without_generators_is_silent() {
        let dir = project("");

        assert!(stale_warnings(Some(dir.path().to_path_buf())).is_empty());
    }

    /// The fresh-clone case: a bridge is configured but absent, because the
    /// default policy keeps it out of the repo.
    #[test]
    fn a_configured_but_absent_bridge_warns() {
        let dir = project(PYTHON_GENERATOR);

        let warnings = stale_warnings(Some(dir.path().to_path_buf()));

        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].contains("has not been generated yet"),
            "{warnings:?}"
        );
    }

    /// A newly initialized project has no generators configured at all, so it
    /// is never nagged before opting in.
    #[test]
    fn a_project_with_no_generators_is_never_nagged() {
        let dir = project("");

        assert!(stale_warnings(Some(dir.path().to_path_buf())).is_empty());
    }

    #[test]
    fn a_current_bridge_is_silent() {
        let dir = project(PYTHON_GENERATOR);
        generate(dir.path(), &current_fingerprint(dir.path()));

        assert!(stale_warnings(Some(dir.path().to_path_buf())).is_empty());
    }

    #[test]
    fn an_out_of_date_bridge_warns_with_the_fix() {
        let dir = project(PYTHON_GENERATOR);
        generate(dir.path(), "a stale fingerprint");

        let warnings = stale_warnings(Some(dir.path().to_path_buf()));

        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(
            warnings[0],
            "generated bridge `client1` is out of date; run `baml bridge generate`"
        );
    }

    /// The passive check reads the manifest, not the generated files, so a
    /// hand-edited bridge is left for `--check` to catch.
    #[test]
    fn a_hand_edited_bridge_is_left_to_the_explicit_check() {
        let dir = project(PYTHON_GENERATOR);
        let fingerprint = current_fingerprint(dir.path());
        generate(dir.path(), &fingerprint);
        let (generators, _) = crate::generate::discover_generators(dir.path());
        fs::write(generators[0].output_dir.join("value.py"), "tampered").unwrap();

        assert!(stale_warnings(Some(dir.path().to_path_buf())).is_empty());
    }
}
