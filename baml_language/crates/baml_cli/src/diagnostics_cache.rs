//! Typed (de)hydration for the per-file diagnostics cache (Phase 1).
//!
//! The manifest stores each file's diagnostics as an opaque borsh blob
//! (`bex_cache` never interprets it — the SeededStdlibInterface pattern). This
//! module owns the typed form: a `CachedDiagnostic` mirror keyed by
//! project-root-relative path so a stored diagnostic survives a `FileId`
//! reassignment across compiles, and the projections between it and the live
//! [`Diagnostic`].
//!
//! Soundness rests on two rules:
//! - A diagnostic whose primary span (or any span) can't be mapped to a tracked
//!   user file — e.g. it points into a `<builtin>/` stub — is **not** cacheable;
//!   its owner file is poisoned so the next compile re-checks it rather than
//!   serving an incomplete set.
//! - Any decode / rehydrate failure degrades to a re-check; the cache never
//!   serves a partial or stale set.

use std::{collections::BTreeMap, path::Path};

use baml_db::{
    FileId, Span,
    baml_compiler_diagnostics::{
        Diagnostic, DiagnosticId, DiagnosticPhase, Severity,
        diagnostic::{Annotation, RelatedInfo},
    },
};
use baml_project::ProjectDatabase;
use text_size::{TextRange, TextSize};

/// A source span keyed by project-root-relative path + byte range, so it is
/// stable across the per-process `FileId` reassignment. Each span stores its
/// own path: a cross-file secondary annotation carries a different file.
#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
struct CachedSpan {
    rel_path: String,
    start: u32,
    end: u32,
}

#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
struct CachedAnnotation {
    span: CachedSpan,
    message: Option<String>,
    is_primary: bool,
}

#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
struct CachedRelatedInfo {
    span: CachedSpan,
    message: String,
    file_path: Option<String>,
}

#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
struct CachedDiagnostic {
    id: DiagnosticId,
    severity: Severity,
    phase: DiagnosticPhase,
    message: String,
    annotations: Vec<CachedAnnotation>,
    related_info: Vec<CachedRelatedInfo>,
}

/// The manifest blob is `borsh(Option<Vec<CachedDiagnostic>>)`: `None` is a
/// poison marker (a diagnostic pointed at a builtin / untracked file, so the
/// set can't be faithfully cached) that forces the owner file to be re-checked.
type CachedFileBlob = Option<Vec<CachedDiagnostic>>;

/// Project-root-relative path for a span's file, or `None` for a builtin /
/// untracked file (which makes the whole diagnostic uncacheable).
fn rel_path_of(db: &ProjectDatabase, root: &Path, file_id: FileId) -> Option<String> {
    let path = db.file_id_to_path(file_id)?;
    if path.to_string_lossy().starts_with("<builtin>/") {
        return None;
    }
    Some(
        path.strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string(),
    )
}

/// The file that owns a diagnostic for per-file caching: its primary span's
/// file, falling back to the first annotation's file (matching the renderer's
/// report anchor). `None` when neither maps to a tracked user file.
fn owner_rel(db: &ProjectDatabase, root: &Path, diag: &Diagnostic) -> Option<String> {
    let span = diag
        .primary_span()
        .or_else(|| diag.annotations.first().map(|a| a.span))?;
    rel_path_of(db, root, span.file_id)
}

fn dehydrate_span(db: &ProjectDatabase, root: &Path, span: Span) -> Option<CachedSpan> {
    Some(CachedSpan {
        rel_path: rel_path_of(db, root, span.file_id)?,
        start: span.range.start().into(),
        end: span.range.end().into(),
    })
}

/// Project a live diagnostic into its rel-path-keyed cache form. `None` if any
/// span can't be mapped to a tracked user file — the whole diagnostic is then
/// uncacheable (its owner file is poisoned by the caller).
fn dehydrate(db: &ProjectDatabase, root: &Path, diag: &Diagnostic) -> Option<CachedDiagnostic> {
    let mut annotations = Vec::with_capacity(diag.annotations.len());
    for a in &diag.annotations {
        annotations.push(CachedAnnotation {
            span: dehydrate_span(db, root, a.span)?,
            message: a.message.clone(),
            is_primary: a.is_primary,
        });
    }
    let mut related_info = Vec::with_capacity(diag.related_info.len());
    for r in &diag.related_info {
        related_info.push(CachedRelatedInfo {
            span: dehydrate_span(db, root, r.span)?,
            message: r.message.clone(),
            file_path: r.file_path.clone(),
        });
    }
    Some(CachedDiagnostic {
        id: diag.id,
        severity: diag.severity,
        phase: diag.phase,
        message: diag.message.clone(),
        annotations,
        related_info,
    })
}

fn rehydrate_span(db: &ProjectDatabase, root: &Path, span: &CachedSpan) -> Option<Span> {
    let full = root.join(&span.rel_path);
    let file_id = db.path_to_file_id(&full)?;
    // Guard against an out-of-range span (the file shrank): the renderer would
    // otherwise index past the source. Degrade to a re-check instead.
    let text_len = db.get_file(&full).map(|sf| sf.text(db).len() as u32)?;
    if span.end > text_len || span.start > span.end {
        return None;
    }
    Some(Span {
        file_id,
        range: TextRange::new(TextSize::new(span.start), TextSize::new(span.end)),
    })
}

/// Rebuild a live diagnostic with current-process `FileId`s. `None` if any span
/// no longer maps to a tracked file or is out of range — the caller degrades to
/// a re-check.
fn rehydrate(db: &ProjectDatabase, root: &Path, cached: &CachedDiagnostic) -> Option<Diagnostic> {
    let mut annotations = Vec::with_capacity(cached.annotations.len());
    for a in &cached.annotations {
        annotations.push(Annotation {
            span: rehydrate_span(db, root, &a.span)?,
            message: a.message.clone(),
            is_primary: a.is_primary,
        });
    }
    let mut related_info = Vec::with_capacity(cached.related_info.len());
    for r in &cached.related_info {
        related_info.push(RelatedInfo {
            span: rehydrate_span(db, root, &r.span)?,
            message: r.message.clone(),
            file_path: r.file_path.clone(),
        });
    }
    Some(Diagnostic {
        id: cached.id,
        severity: cached.severity,
        message: cached.message.clone(),
        annotations,
        related_info,
        phase: cached.phase,
    })
}

/// Serialize one file's diagnostics into the opaque manifest blob. Returns a
/// poison blob (`None` inside) if any diagnostic can't be dehydrated, so the
/// owner file is conservatively re-checked next compile rather than served an
/// incomplete set.
fn serialize_file_blob(db: &ProjectDatabase, root: &Path, diags: &[&Diagnostic]) -> Vec<u8> {
    let mut cached = Vec::with_capacity(diags.len());
    for d in diags {
        match dehydrate(db, root, d) {
            Some(c) => cached.push(c),
            None => return borsh::to_vec(&CachedFileBlob::None).unwrap_or_default(),
        }
    }
    borsh::to_vec(&CachedFileBlob::Some(cached)).unwrap_or_default()
}

/// The blob for a checked file with no diagnostics (distinct from a poison
/// blob, which forces a re-check).
pub(crate) fn empty_blob() -> Vec<u8> {
    borsh::to_vec(&CachedFileBlob::Some(Vec::<CachedDiagnostic>::new())).unwrap_or_default()
}

/// Test helper: a valid blob fabricating one error at bytes 0..1 of `rel_path`.
/// It rehydrates cleanly (the span is in range for any non-empty file), so
/// comparing it against an honest check of an error-free file yields a genuine
/// non-empty-vs-empty mismatch — used to prove the sampled-verify tripwire
/// fires on a stale diagnostics blob.
#[cfg(test)]
pub(crate) fn one_fake_diagnostic_blob(rel_path: &str) -> Vec<u8> {
    let cached = vec![CachedDiagnostic {
        id: DiagnosticId::TypeMismatch,
        severity: Severity::Error,
        phase: DiagnosticPhase::Type,
        message: "sampled-verify poison".to_string(),
        annotations: vec![CachedAnnotation {
            span: CachedSpan {
                rel_path: rel_path.to_string(),
                start: 0,
                end: 1,
            },
            message: None,
            is_primary: true,
        }],
        related_info: Vec::new(),
    }];
    borsh::to_vec(&CachedFileBlob::Some(cached)).unwrap_or_default()
}

/// Rehydrate an opaque manifest blob into live diagnostics. `None` — a poison
/// marker, an undecodable blob, or an unmappable / out-of-range span — means
/// "re-check this file"; the cache never serves a partial or stale set.
pub(crate) fn rehydrate_file_blob(
    db: &ProjectDatabase,
    root: &Path,
    blob: &[u8],
) -> Option<Vec<Diagnostic>> {
    let cached: CachedFileBlob = borsh::from_slice(blob).ok()?;
    let cached = cached?;
    let mut out = Vec::with_capacity(cached.len());
    for c in &cached {
        out.push(rehydrate(db, root, c)?);
    }
    Some(out)
}

/// Group freshly-checked diagnostics by owner file and serialize each group to
/// its manifest blob. Only `check_file` output belongs here — the package-level
/// set is never cached. A diagnostic with no tracked owner file is dropped
/// (it renders at a sentinel span and is not attributable to a file).
pub(crate) fn fresh_blobs_by_file(
    db: &ProjectDatabase,
    root: &Path,
    fresh: &[Diagnostic],
) -> BTreeMap<String, Vec<u8>> {
    let mut groups: BTreeMap<String, Vec<&Diagnostic>> = BTreeMap::new();
    for d in fresh {
        if let Some(rel) = owner_rel(db, root, d) {
            groups.entry(rel).or_default().push(d);
        }
    }
    groups
        .into_iter()
        .map(|(rel, diags)| (rel, serialize_file_blob(db, root, &diags)))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use baml_db::baml_compiler_diagnostics::render;
    use text_size::TextRange;

    use super::*;

    fn span_in(db: &ProjectDatabase, path: &Path, start: u32, end: u32) -> Span {
        let file_id = db.path_to_file_id(path).expect("file present");
        Span {
            file_id,
            range: TextRange::new(TextSize::new(start), TextSize::new(end)),
        }
    }

    fn build_db(files: &[(&str, &str)]) -> (ProjectDatabase, std::path::PathBuf) {
        let root = std::path::PathBuf::from("/diag-cache-test");
        let mut db = ProjectDatabase::new();
        db.set_project_root(&root);
        for (name, content) in files {
            db.add_or_update_file(&root.join(name), content);
        }
        (db, root)
    }

    #[test]
    fn round_trip_with_cross_file_annotation_and_related_info() {
        let (db, root) = build_db(&[
            ("a.baml", "function a() -> int {\n  1\n}\n"),
            ("b.baml", "function b() -> int {\n  2\n}\n"),
        ]);
        let a = root.join("a.baml");
        let b = root.join("b.baml");

        // Primary in a.baml, a secondary annotation and related_info in b.baml.
        let diag = Diagnostic::error(DiagnosticId::DuplicateName, "duplicate `x`")
            .with_primary_span(span_in(&db, &a, 0, 8))
            .with_secondary(span_in(&db, &b, 9, 17), "also here")
            .with_related(span_in(&db, &b, 0, 4), "first here");

        let cached = dehydrate(&db, &root, &diag).expect("dehydrates");
        let bytes = borsh::to_vec(&CachedFileBlob::Some(vec![cached])).unwrap();
        let restored = rehydrate_file_blob(&db, &root, &bytes).expect("rehydrates");
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0], diag, "round-trip preserves the diagnostic");

        // And it renders byte-identically to the original.
        let mut sources = std::collections::HashMap::new();
        let mut paths = std::collections::HashMap::new();
        for p in [&a, &b] {
            let sf = db.get_file(p).unwrap();
            sources.insert(sf.file_id(&db), sf.text(&db).to_string());
            paths.insert(sf.file_id(&db), p.clone());
        }
        let cfg = render::RenderConfig::cli_auto();
        assert_eq!(
            render::render_diagnostics(&[diag], &sources, &paths, &cfg),
            render::render_diagnostics(&restored, &sources, &paths, &cfg),
            "cached diagnostic renders identically"
        );
    }

    #[test]
    fn dehydrate_declines_builtin_span() {
        let (db, root) = build_db(&[("a.baml", "function a() -> int {\n  1\n}\n")]);
        // A span pointing at a builtin file id can't be mapped to a rel_path.
        let builtin_file = baml_db::baml_compiler2_hir::compiler2_all_files(&db)
            .into_iter()
            .find(|sf| sf.path(&db).to_string_lossy().starts_with("<builtin>/"))
            .expect("a builtin file exists");
        let builtin_span = Span {
            file_id: builtin_file.file_id(&db),
            range: TextRange::new(TextSize::new(0), TextSize::new(3)),
        };
        let diag =
            Diagnostic::error(DiagnosticId::TypeMismatch, "bad").with_primary_span(builtin_span);
        assert!(
            dehydrate(&db, &root, &diag).is_none(),
            "a builtin-pointing diagnostic is uncacheable"
        );
        // And serializing a group containing it yields the poison blob, which
        // rehydrates to `None` (re-check).
        let blob = serialize_file_blob(&db, &root, &[&diag]);
        assert!(
            rehydrate_file_blob(&db, &root, &blob).is_none(),
            "poison degrades"
        );
    }

    #[test]
    fn rehydrate_degrades_on_missing_path_and_out_of_range() {
        let (db, root) = build_db(&[("a.baml", "function a() -> int {\n  1\n}\n")]);
        let a = root.join("a.baml");

        // A cached diagnostic naming a path absent from this db → None.
        let ghost = CachedDiagnostic {
            id: DiagnosticId::TypeMismatch,
            severity: Severity::Error,
            phase: DiagnosticPhase::Type,
            message: "x".to_string(),
            annotations: vec![CachedAnnotation {
                span: CachedSpan {
                    rel_path: "gone.baml".to_string(),
                    start: 0,
                    end: 1,
                },
                message: None,
                is_primary: true,
            }],
            related_info: Vec::new(),
        };
        let blob = borsh::to_vec(&CachedFileBlob::Some(vec![ghost])).unwrap();
        assert!(
            rehydrate_file_blob(&db, &root, &blob).is_none(),
            "missing path degrades"
        );

        // An out-of-range span (past the current file length) → None.
        let text_len = db.get_file(&a).unwrap().text(&db).len() as u32;
        let oob = CachedDiagnostic {
            id: DiagnosticId::TypeMismatch,
            severity: Severity::Error,
            phase: DiagnosticPhase::Type,
            message: "x".to_string(),
            annotations: vec![CachedAnnotation {
                span: CachedSpan {
                    rel_path: "a.baml".to_string(),
                    start: 0,
                    end: text_len + 100,
                },
                message: None,
                is_primary: true,
            }],
            related_info: Vec::new(),
        };
        let blob = borsh::to_vec(&CachedFileBlob::Some(vec![oob])).unwrap();
        assert!(
            rehydrate_file_blob(&db, &root, &blob).is_none(),
            "out-of-range span degrades"
        );
    }

    #[test]
    fn undecodable_blob_degrades() {
        let (db, root) = build_db(&[("a.baml", "function a() -> int {\n  1\n}\n")]);
        assert!(
            rehydrate_file_blob(&db, &root, &[]).is_none(),
            "an empty/legacy blob is undecodable -> re-check"
        );
        assert!(
            rehydrate_file_blob(&db, &root, &empty_blob())
                .unwrap()
                .is_empty(),
            "an explicit empty blob rehydrates to no diagnostics"
        );
    }
}
