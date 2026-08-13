//! The one place that decides whether a generated bridge is out of date.
//!
//! `bridge generate --check`, `bridge list`, and the passive warning all call
//! [`evaluate`], so they cannot disagree about what "stale" means. They differ
//! only in [`Depth`] and in how they present the answer.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use baml_codegen_types::{
    FileDrift, ProvenanceStatus, read_output_provenance, verify_recorded_files,
};

/// How hard to look.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Depth {
    /// Compare the recorded toolchain version and input fingerprint. One
    /// small file read, so this is what the passive check on ordinary
    /// commands can afford.
    Manifest,
    /// Also re-hash every recorded file, catching a hand-edited, truncated,
    /// or deleted bridge whose inputs never changed.
    VerifyFiles,
}

/// Why a bridge is out of date. A single check can report several.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Reason {
    ToolchainSkew {
        generated_by: String,
        current: String,
    },
    InputsChanged,
    /// The bridge records no provenance, so it cannot be vouched for.
    ProvenanceMissing,
    Modified(Vec<PathBuf>),
    Missing(Vec<PathBuf>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Status {
    /// No manifest: this bridge has never been generated. Distinct from
    /// stale, because a freshly configured project has not opted in yet.
    NeverGenerated,
    Fresh,
    Stale(Vec<Reason>),
}

impl Status {
    pub(crate) fn is_stale(&self) -> bool {
        matches!(self, Self::Stale(_))
    }
}

/// Compare a generated bridge against the inputs it should have been built
/// from. Never compiles, and never regenerates to diff.
pub(crate) fn evaluate(
    generated_directory: &Path,
    expected_fingerprint: &str,
    toolchain_version: &str,
    depth: Depth,
) -> Result<Status> {
    let provenance = read_output_provenance(generated_directory).with_context(|| {
        format!(
            "failed to read the generated bridge in {}",
            generated_directory.display()
        )
    })?;

    let provenance = match provenance {
        ProvenanceStatus::NeverGenerated => return Ok(Status::NeverGenerated),
        // Not a third answer: a bridge we cannot vouch for needs regenerating.
        ProvenanceStatus::Untracked => {
            return Ok(Status::Stale(vec![Reason::ProvenanceMissing]));
        }
        ProvenanceStatus::Known(provenance) => provenance,
    };

    let mut reasons = Vec::new();
    // Reported before the fingerprint, and separately from it, so version
    // skew reads as version skew rather than as an opaque hash mismatch.
    if provenance.toolchain_version != toolchain_version {
        reasons.push(Reason::ToolchainSkew {
            generated_by: provenance.toolchain_version.clone(),
            current: toolchain_version.to_string(),
        });
    }
    if provenance.input_fingerprint != expected_fingerprint {
        reasons.push(Reason::InputsChanged);
    }

    if depth == Depth::VerifyFiles {
        let drift = verify_recorded_files(generated_directory).with_context(|| {
            format!(
                "failed to verify the generated bridge in {}",
                generated_directory.display()
            )
        })?;
        let (modified, missing): (Vec<_>, Vec<_>) = drift
            .into_iter()
            .partition(|entry| matches!(entry, FileDrift::Modified(_)));
        if !modified.is_empty() {
            reasons.push(Reason::Modified(
                modified
                    .iter()
                    .map(|entry| entry.path().to_path_buf())
                    .collect(),
            ));
        }
        if !missing.is_empty() {
            reasons.push(Reason::Missing(
                missing
                    .iter()
                    .map(|entry| entry.path().to_path_buf())
                    .collect(),
            ));
        }
    }

    if reasons.is_empty() {
        Ok(Status::Fresh)
    } else {
        Ok(Status::Stale(reasons))
    }
}

/// One-line explanation, in the voice of the existing `FORMAT_HINT`:
/// lowercase, no emoji, the fix spelled out.
pub(crate) fn warning(generator_name: &str, reasons: &[Reason]) -> String {
    let detail = reasons.first().map_or_else(
        || "is out of date".to_string(),
        |reason| match reason {
            Reason::ToolchainSkew {
                generated_by,
                current,
            } => format!("was built by BAML {generated_by} but this toolchain is {current}"),
            Reason::InputsChanged => "is out of date".to_string(),
            Reason::ProvenanceMissing => {
                "was built before BAML tracked bridge freshness".to_string()
            }
            Reason::Modified(paths) => format!("has hand-edited files ({})", summarize(paths)),
            Reason::Missing(paths) => format!("is missing files ({})", summarize(paths)),
        },
    );
    format!("generated bridge `{generator_name}` {detail}; run `baml bridge generate`")
}

/// Name at most two paths, then count the rest, so a wholesale change does
/// not print hundreds of lines.
fn summarize(paths: &[PathBuf]) -> String {
    let named = paths
        .iter()
        .take(2)
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    match paths.len().saturating_sub(2) {
        0 => named,
        rest => format!("{named}, and {rest} more"),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use baml_codegen_types::{
        GeneratedOutputFile, OutputOptions, OutputProvenance, VcsPolicy, write_generated_output,
    };

    use super::*;

    const TOOLCHAIN: &str = "0.15.0";

    /// A generated bridge recorded as built from `fingerprint`.
    fn bridge(directory: &Path, fingerprint: &str, toolchain: &str) {
        write_generated_output(
            directory,
            vec![GeneratedOutputFile::new("value.py", "generated")],
            &OutputOptions {
                provenance: OutputProvenance {
                    input_fingerprint: fingerprint.to_string(),
                    toolchain_version: toolchain.to_string(),
                    generator_name: "client1".to_string(),
                },
                vcs: VcsPolicy::Ignore,
            },
        )
        .unwrap();
    }

    #[test]
    fn matching_inputs_and_toolchain_are_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("baml_sdk");
        bridge(&output, "abc", TOOLCHAIN);

        assert_eq!(
            evaluate(&output, "abc", TOOLCHAIN, Depth::VerifyFiles).unwrap(),
            Status::Fresh
        );
    }

    #[test]
    fn a_changed_fingerprint_is_stale() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("baml_sdk");
        bridge(&output, "abc", TOOLCHAIN);

        let status = evaluate(&output, "def", TOOLCHAIN, Depth::Manifest).unwrap();

        assert_eq!(status, Status::Stale(vec![Reason::InputsChanged]));
    }

    #[test]
    fn version_skew_is_reported_as_version_skew() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("baml_sdk");
        bridge(&output, "abc", "0.14.2");

        let status = evaluate(&output, "abc", TOOLCHAIN, Depth::Manifest).unwrap();

        let Status::Stale(reasons) = &status else {
            panic!("expected stale, got {status:?}");
        };
        assert_eq!(
            reasons[0],
            Reason::ToolchainSkew {
                generated_by: "0.14.2".to_string(),
                current: TOOLCHAIN.to_string(),
            }
        );
        assert!(
            warning("client1", reasons)
                .contains("was built by BAML 0.14.2 but this toolchain is 0.15.0")
        );
    }

    /// The inputs are untouched, so only the deep check can catch this.
    #[test]
    fn a_hand_edited_file_is_caught_only_at_verify_depth() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("baml_sdk");
        bridge(&output, "abc", TOOLCHAIN);
        fs::write(output.join("value.py"), "hand edited").unwrap();

        assert_eq!(
            evaluate(&output, "abc", TOOLCHAIN, Depth::Manifest).unwrap(),
            Status::Fresh
        );
        let status = evaluate(&output, "abc", TOOLCHAIN, Depth::VerifyFiles).unwrap();
        assert_eq!(
            status,
            Status::Stale(vec![Reason::Modified(vec![PathBuf::from("value.py")])])
        );
    }

    #[test]
    fn a_deleted_file_is_caught_at_verify_depth() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("baml_sdk");
        bridge(&output, "abc", TOOLCHAIN);
        fs::remove_file(output.join("value.py")).unwrap();

        let status = evaluate(&output, "abc", TOOLCHAIN, Depth::VerifyFiles).unwrap();

        assert_eq!(
            status,
            Status::Stale(vec![Reason::Missing(vec![PathBuf::from("value.py")])])
        );
    }

    #[test]
    fn an_absent_bridge_is_never_generated_rather_than_stale() {
        let dir = tempfile::tempdir().unwrap();

        let status = evaluate(
            &dir.path().join("baml_sdk"),
            "abc",
            TOOLCHAIN,
            Depth::VerifyFiles,
        )
        .unwrap();

        assert_eq!(status, Status::NeverGenerated);
        assert!(!status.is_stale());
    }

    /// A bridge with no recorded provenance cannot be vouched for, so it
    /// reads as stale rather than as a third "passed but unverified" answer.
    #[test]
    fn a_manifest_without_provenance_is_stale() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("baml_sdk");
        bridge(&output, "abc", TOOLCHAIN);
        // Strip provenance, leaving an otherwise valid schema-2 manifest.
        let manifest_path = output.join(".baml-generator-output.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        let table = manifest.as_object_mut().unwrap();
        table.insert("schema_version".to_string(), serde_json::json!(2));
        table.remove("input_fingerprint");
        table.remove("toolchain_version");
        table.remove("generator_name");
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();

        let status = evaluate(&output, "totally different", TOOLCHAIN, Depth::VerifyFiles).unwrap();

        assert_eq!(status, Status::Stale(vec![Reason::ProvenanceMissing]));
        assert!(status.is_stale());
    }

    #[test]
    fn many_drifted_files_are_summarized() {
        let paths = vec![
            PathBuf::from("a.py"),
            PathBuf::from("b.py"),
            PathBuf::from("c.py"),
            PathBuf::from("d.py"),
        ];

        assert_eq!(summarize(&paths), "a.py, b.py, and 2 more");
        assert_eq!(summarize(&paths[..1]), "a.py");
    }
}
